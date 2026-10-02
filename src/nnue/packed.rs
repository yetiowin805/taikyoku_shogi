//! Exact byte dot products for the first dense layer, with a portable fallback.

fn scalar(x: &[u8], weights: &[i8]) -> i32 {
    x.iter()
        .zip(weights)
        .map(|(&a, &b)| i32::from(a) * i32::from(b))
        .sum()
}

// The caller supplies at most 8192 inputs, each in 0..=127. Thus maddubs'
// pair sums are in [-32512, 32258], so its saturating step is exact. The full
// dot product also fits i32. Unaligned loads require no allocation alignment.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn avx2(x: &[u8], weights: &[i8]) -> i32 {
    use std::arch::x86_64::*;
    let mut acc = _mm256_setzero_si256();
    let ones = _mm256_set1_epi16(1);
    for i in (0..x.len()).step_by(32) {
        let a = _mm256_loadu_si256(x.as_ptr().add(i).cast());
        let b = _mm256_loadu_si256(weights.as_ptr().add(i).cast());
        let pairs = _mm256_maddubs_epi16(a, b);
        acc = _mm256_add_epi32(acc, _mm256_madd_epi16(pairs, ones));
    }
    let mut lanes = [0i32; 8];
    _mm256_storeu_si256(lanes.as_mut_ptr().cast(), acc);
    lanes.into_iter().sum()
}

// All 32 first-layer outputs. VNNI's vpdpbusd adds four exact u8*i8 products
// per lane without saturation, so it matches the scalar sum exactly.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f,avx512bw,avx512vnni")]
unsafe fn vnni32(x: &[u8], h1: &[i8], out: &mut [i32; 32]) {
    use std::arch::x86_64::*;
    let n = x.len();
    for j in (0..32).step_by(4) {
        let (mut a0, mut a1, mut a2, mut a3) = (
            _mm512_setzero_si512(),
            _mm512_setzero_si512(),
            _mm512_setzero_si512(),
            _mm512_setzero_si512(),
        );
        let r = h1.as_ptr().add(j * n);
        for i in (0..n).step_by(64) {
            let xa = _mm512_loadu_si512(x.as_ptr().add(i).cast());
            a0 = _mm512_dpbusd_epi32(a0, xa, _mm512_loadu_si512(r.add(i).cast()));
            a1 = _mm512_dpbusd_epi32(a1, xa, _mm512_loadu_si512(r.add(n + i).cast()));
            a2 = _mm512_dpbusd_epi32(a2, xa, _mm512_loadu_si512(r.add(2 * n + i).cast()));
            a3 = _mm512_dpbusd_epi32(a3, xa, _mm512_loadu_si512(r.add(3 * n + i).cast()));
        }
        out[j] = _mm512_reduce_add_epi32(a0);
        out[j + 1] = _mm512_reduce_add_epi32(a1);
        out[j + 2] = _mm512_reduce_add_epi32(a2);
        out[j + 3] = _mm512_reduce_add_epi32(a3);
    }
}

// AVX-VNNI also provides exact byte dot products on CPUs without AVX-512.
// Reuse each input vector across four output rows to reduce load overhead.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,avxvnni")]
unsafe fn avx_vnni32(x: &[u8], h1: &[i8], out: &mut [i32; 32]) {
    use std::arch::x86_64::*;
    let n = x.len();
    for j in (0..32).step_by(4) {
        let mut acc = [_mm256_setzero_si256(); 4];
        for i in (0..n).step_by(32) {
            let input = _mm256_loadu_si256(x.as_ptr().add(i).cast());
            for k in 0..4 {
                let weights = _mm256_loadu_si256(h1.as_ptr().add((j + k) * n + i).cast());
                acc[k] = _mm256_dpbusd_avx_epi32(acc[k], input, weights);
            }
        }
        for k in 0..4 {
            let mut lanes = [0i32; 8];
            _mm256_storeu_si256(lanes.as_mut_ptr().cast(), acc[k]);
            out[j + k] = lanes.into_iter().sum();
        }
    }
}

pub(super) fn affine32(x: &[u8], h1: &[i8], out: &mut [i32; 32]) {
    let n = x.len();
    assert_eq!(h1.len(), 32 * n);
    assert_eq!(n % 64, 0);
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx512vnni") && is_x86_feature_detected!("avx512bw") {
        // SAFETY: runtime detection; lengths asserted above.
        unsafe { vnni32(x, h1, out) };
        return;
    }
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avxvnni") && is_x86_feature_detected!("avx2") {
        // SAFETY: runtime detection; validated row lengths are multiples of 64.
        unsafe { avx_vnni32(x, h1, out) };
        return;
    }
    for (j, o) in out.iter_mut().enumerate() {
        *o = dot(x, &h1[j * n..(j + 1) * n]);
    }
}

pub(super) fn dot(x: &[u8], weights: &[i8]) -> i32 {
    assert_eq!(x.len(), weights.len());
    assert_eq!(x.len() % 32, 0);
    debug_assert!(x.len() <= 8192 && x.iter().all(|&a| a <= 127));
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        // SAFETY: runtime detection guarantees AVX2; lengths and input bounds
        // come from the validated network and the clamped activation vector.
        return unsafe { avx2(x, weights) };
    }
    scalar(x, weights)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_and_scalar_agree_at_extremes_and_supported_widths() {
        for width in [32, 512, 768, 1024, 1536, 2048, 4096] {
            for mode in 0..4 {
                let x: Vec<_> = (0..2 * width)
                    .map(|i| if mode == 0 { (i % 128) as u8 } else { 127 })
                    .collect();
                let weights: Vec<_> = (0..x.len())
                    .map(|i| match mode {
                        0 => ((i * 73) % 256) as u8 as i8,
                        1 => -128,
                        2 => 127,
                        _ => {
                            if i % 2 == 0 {
                                -128
                            } else {
                                127
                            }
                        }
                    })
                    .collect();
                assert_eq!(dot(&x, &weights), scalar(&x, &weights));
                let h1: Vec<i8> = (0..32)
                    .flat_map(|j| {
                        weights.iter().map(move |&b| {
                            if j % 3 == 0 {
                                b
                            } else {
                                b.wrapping_mul(j as i8 | 1)
                            }
                        })
                    })
                    .collect();
                let mut got = [0i32; 32];
                affine32(&x, &h1, &mut got);
                for j in 0..32 {
                    assert_eq!(got[j], scalar(&x, &h1[j * x.len()..(j + 1) * x.len()]));
                }
            }
        }
    }
}
