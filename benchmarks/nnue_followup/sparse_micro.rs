//! Standalone rustc experiment using quantized activations sampled from real searches.
use std::{
    hint::black_box,
    io::{Read, Seek, SeekFrom},
    time::Instant,
};
#[path = "../../src/nnue/packed.rs"]
mod packed;

#[target_feature(enable = "avx2,avxvnni")]
unsafe fn sparse(x: &[u8], transposed: &[i8], out: &mut [i32; 32]) {
    use std::arch::x86_64::*;
    let mut sums = [_mm256_setzero_si256(); 4];
    for (group, xs) in x.chunks_exact(4).enumerate() {
        let value = i32::from_le_bytes(xs.try_into().unwrap());
        if value == 0 {
            continue;
        }
        let input = _mm256_set1_epi32(value);
        for k in 0..4 {
            let weights = _mm256_loadu_si256(transposed.as_ptr().add(group * 128 + k * 32).cast());
            sums[k] = _mm256_dpbusd_avx_epi32(sums[k], input, weights);
        }
    }
    for k in 0..4 {
        _mm256_storeu_si256(out.as_mut_ptr().add(k * 8).cast(), sums[k]);
    }
}

fn main() {
    assert!(is_x86_feature_detected!("avxvnni") && is_x86_feature_detected!("avx2"));
    let args: Vec<_> = std::env::args().collect();
    let w: usize = args[1].parse().unwrap();
    let mut net = std::fs::File::open(&args[2]).unwrap();
    net.seek(SeekFrom::Start((64 + 92 * 1296 * 2 * w * 2 + w * 4) as u64))
        .unwrap();
    let mut raw = vec![0u8; 2 * w * 32];
    net.read_exact(&mut raw).unwrap();
    let h1: Vec<i8> = raw.into_iter().map(|v| v as i8).collect();
    let mut transposed = vec![0i8; h1.len()];
    for group in 0..2 * w / 4 {
        for output in 0..32 {
            for k in 0..4 {
                transposed[group * 128 + output * 4 + k] = h1[output * 2 * w + group * 4 + k];
            }
        }
    }
    let samples = std::fs::read(&args[3]).unwrap();
    assert_eq!(samples.len() % (2 * w), 0);
    for x in samples.chunks_exact(2 * w) {
        let (mut dense_out, mut sparse_out) = ([0; 32], [0; 32]);
        packed::affine32(x, &h1, &mut dense_out);
        unsafe {
            sparse(x, &transposed, &mut sparse_out);
        }
        assert_eq!(dense_out, sparse_out);
    }
    let mut result = [0; 32];
    // Alternating order avoids attributing a gradual frequency drift to one kernel.
    for rep in 0..6 {
        for which in if rep % 2 == 0 { [0, 1] } else { [1, 0] } {
            let start = Instant::now();
            for _ in 0..300 {
                for x in samples.chunks_exact(2 * w) {
                    if which == 0 {
                        packed::affine32(black_box(x), black_box(&h1), &mut result);
                    } else {
                        unsafe {
                            sparse(black_box(x), black_box(&transposed), &mut result);
                        }
                    }
                    black_box(result);
                }
            }
            println!(
                "{{\"width\":{w},\"rep\":{rep},\"sparse\":{},\"elapsed_ns\":{},\"samples\":{}}}",
                which == 1,
                start.elapsed().as_nanos(),
                samples.len() / (2 * w)
            );
        }
    }
}
