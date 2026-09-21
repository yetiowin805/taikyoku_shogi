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
            }
        }
    }
}
