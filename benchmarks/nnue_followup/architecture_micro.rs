//! Head-layout and cached-update cost only. No search, training, or quality claim.
//! Inputs come from real search activations; changed heads use synthetic weights.
use std::{hint::black_box, time::Instant};

#[derive(Clone, Copy)]
struct Shape {
    name: &'static str,
    width: usize,
    perspectives: usize,
    hidden: usize,
    pairwise: bool,
    psqt: bool,
}

#[target_feature(enable = "avx2,avxvnni")]
unsafe fn dense(x: &[u8], h1: &[i8], hidden: usize, out: &mut [i32; 32]) {
    use std::arch::x86_64::*;
    let n = x.len();
    for j in (0..hidden).step_by(4) {
        let mut sums = [_mm256_setzero_si256(); 4];
        for i in (0..n).step_by(32) {
            let input = _mm256_loadu_si256(x.as_ptr().add(i).cast());
            for k in 0..4 {
                let weights = _mm256_loadu_si256(h1.as_ptr().add((j + k) * n + i).cast());
                sums[k] = _mm256_dpbusd_avx_epi32(sums[k], input, weights);
            }
        }
        for k in 0..4 {
            let mut lanes = [0i32; 8];
            _mm256_storeu_si256(lanes.as_mut_ptr().cast(), sums[k]);
            out[j + k] = lanes.into_iter().sum();
        }
    }
}

struct Net {
    shape: Shape,
    h1: Vec<i8>,
    h2: Vec<i8>,
    output: [f32; 32],
}

impl Net {
    fn new(shape: Shape) -> Self {
        let mut seed = 23112026u64;
        let mut weight = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed as u8 as i8) / 4
        };
        let n = shape.width * shape.perspectives / if shape.pairwise { 2 } else { 1 };
        Self {
            shape,
            h1: (0..n * shape.hidden).map(|_| weight()).collect(),
            h2: (0..32 * shape.hidden).map(|_| weight()).collect(),
            output: std::array::from_fn(|_| f32::from(weight()) / 32.),
        }
    }
    fn forward(&self, original: &[u8], input: &mut Vec<u8>, psqt: &[i32; 8]) -> f32 {
        let s = self.shape;
        let per_side = s.width / if s.pairwise { 2 } else { 1 };
        input.resize(per_side * s.perspectives, 0);
        for side in 0..s.perspectives {
            let x = &original[side * 2048..side * 2048 + s.width];
            let dst = &mut input[side * per_side..(side + 1) * per_side];
            if s.pairwise {
                // Fixed-point product: shifts and contiguous destinations allow
                // vectorization. A trained architecture would use this scale.
                for ((d, &a), &b) in dst.iter_mut().zip(&x[..per_side]).zip(&x[per_side..]) {
                    *d = ((u16::from(a) * u16::from(b) + 64) >> 7) as u8;
                }
            } else {
                dst.copy_from_slice(x);
            }
        }
        let mut h = [0i32; 32];
        unsafe { dense(input, &self.h1, s.hidden, &mut h) };
        for v in &mut h[..s.hidden] {
            *v = ((*v + 32) / 64).clamp(0, 127);
        }
        let mut result = 0.;
        for (row, &weight) in self.h2.chunks_exact(s.hidden).zip(&self.output) {
            let value: i32 = row
                .iter()
                .zip(&h[..s.hidden])
                .map(|(&w, &x)| i32::from(w) * x)
                .sum();
            result += ((value + 32) / 64).clamp(0, 127) as f32 / 127. * weight;
        }
        if s.psqt {
            // Output cost of four already-accumulated linear lanes per perspective.
            result +=
                (psqt[..4].iter().sum::<i32>() - psqt[4..].iter().sum::<i32>()) as f32 / 4096.;
        }
        result
    }
}

fn main() {
    assert!(is_x86_feature_detected!("avx2") && is_x86_feature_detected!("avxvnni"));
    let path = std::env::args()
        .nth(1)
        .expect("W2048 sampled activation file");
    let samples = std::fs::read(path).unwrap();
    assert_eq!(samples.len() % 4096, 0);
    assert!(!samples.is_empty());
    let base = Shape {
        name: "baseline",
        width: 2048,
        perspectives: 2,
        hidden: 32,
        pairwise: false,
        psqt: false,
    };
    let shapes = [
        base,
        Shape {
            name: "h16",
            hidden: 16,
            ..base
        },
        Shape {
            name: "pairwise",
            pairwise: true,
            ..base
        },
        Shape {
            name: "pairwise-h16",
            pairwise: true,
            hidden: 16,
            ..base
        },
        Shape {
            name: "psqt",
            psqt: true,
            ..base
        },
        Shape {
            name: "pairwise-h16-psqt",
            pairwise: true,
            hidden: 16,
            psqt: true,
            ..base
        },
        Shape {
            name: "single",
            perspectives: 1,
            ..base
        },
        Shape {
            name: "single-pairwise-h16-psqt",
            perspectives: 1,
            pairwise: true,
            hidden: 16,
            psqt: true,
            ..base
        },
        Shape {
            name: "width1024",
            width: 1024,
            ..base
        },
        Shape {
            name: "width512",
            width: 512,
            ..base
        },
    ];
    let nets: Vec<_> = shapes.into_iter().map(Net::new).collect();
    let mut input = Vec::with_capacity(4096);
    let psqt = [271, -43, 519, 713, 899, 61, -5, 137];
    for rep in 0..6 {
        // Balanced reversed order, with a rotating start across repetitions.
        for j in 0..nets.len() {
            let i = if rep % 2 == 0 {
                (j + rep) % nets.len()
            } else {
                (nets.len() - 1 - j + rep) % nets.len()
            };
            let net = &nets[i];
            let start = Instant::now();
            let repeats = 200;
            for _ in 0..repeats {
                for x in samples.chunks_exact(4096) {
                    black_box(net.forward(black_box(x), &mut input, black_box(&psqt)));
                }
            }
            println!("{{\"rep\":{rep},\"variant\":\"{}\",\"phase\":\"head\",\"elapsed_ns\":{},\"calls\":{}}}",net.shape.name,start.elapsed().as_nanos(),repeats*samples.len()/4096);
        }
    }
    // One move-delta cache hit; does not model compulsory weight-row misses.
    for rep in 0..6 {
        for j in 0..=shapes.len() {
            let count = shapes.len() + 1;
            let i = if rep % 2 == 0 {
                (j + rep) % count
            } else {
                (count - 1 - j + rep) % count
            };
            if i == shapes.len() {
                guarded_i16_probe(rep);
                continue;
            }
            let s = shapes[i];
            let n = s.width * s.perspectives;
            let sums: Vec<i32> = (0..n).map(|i| (i as i32 * 13) % 16000 - 8000).collect();
            let delta: Vec<i32> = (0..n).map(|i| (i as i32 * 19) % 2000 - 1000).collect();
            let mut out = vec![0i32; n];
            let mut linear = [0i32; 8];
            let start = Instant::now();
            for _ in 0..100_000 {
                for (o, (&a, &b)) in out
                    .iter_mut()
                    .zip(black_box(&sums).iter().zip(black_box(&delta)))
                {
                    *o = a + b;
                }
                if s.psqt {
                    for k in 0..8 {
                        linear[k] = black_box(psqt[k]) + black_box(psqt[7 - k]);
                    }
                }
                black_box(&out);
                black_box(&linear);
            }
            println!("{{\"rep\":{rep},\"variant\":\"{}\",\"phase\":\"cached_update\",\"elapsed_ns\":{},\"calls\":100000}}",s.name,start.elapsed().as_nanos());
        }
    }
}

// Narrow sums are exact only with an overflow escape. This models one hot
// cached-delta application, not stack conversion, uncached rows or the engine.
#[target_feature(enable = "avx2")]
unsafe fn guarded_add(a: &[i16], b: &[i16], out: &mut [i16]) -> bool {
    use std::arch::x86_64::*;
    let mut overflow = _mm256_setzero_si256();
    for i in (0..a.len()).step_by(16) {
        let av = _mm256_loadu_si256(a.as_ptr().add(i).cast());
        let bv = _mm256_loadu_si256(b.as_ptr().add(i).cast());
        let sum = _mm256_add_epi16(av, bv);
        let saturated = _mm256_adds_epi16(av, bv);
        overflow = _mm256_or_si256(overflow, _mm256_xor_si256(sum, saturated));
        _mm256_storeu_si256(out.as_mut_ptr().add(i).cast(), sum);
    }
    _mm256_testz_si256(overflow, overflow) != 0
}

fn guarded_i16_probe(rep: usize) {
    let a: Vec<i16> = (0..4096)
        .map(|i| ((i * 13) % 16000 - 8000) as i16)
        .collect();
    let b: Vec<i16> = (0..4096).map(|i| ((i * 19) % 2000 - 1000) as i16).collect();
    let mut out = vec![0i16; 4096];
    assert!(unsafe { guarded_add(&a, &b, &mut out) });
    assert!(out
        .iter()
        .zip(a.iter().zip(&b))
        .all(|(&v, (&x, &y))| i32::from(v) == i32::from(x) + i32::from(y)));
    let mut extreme = a.clone();
    extreme[0] = i16::MIN;
    assert!(!unsafe { guarded_add(&extreme, &b, &mut out) });
    let start = Instant::now();
    for _ in 0..100_000 {
        let fits = unsafe { guarded_add(black_box(&a), black_box(&b), black_box(&mut out)) };
        assert!(black_box(fits));
        black_box(&out);
    }
    println!("{{\"rep\":{rep},\"variant\":\"guarded-i16\",\"phase\":\"cached_update\",\"elapsed_ns\":{},\"calls\":100000}}",start.elapsed().as_nanos());
}
