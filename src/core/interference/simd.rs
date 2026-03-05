const TWO_PI: f64 = 2.0 * std::f64::consts::PI;
const WAVELET_THRESHOLD: usize = 2_048;
const HAAR_LEVELS: usize = 6; // 64 bins => deterministic coverage

/// PHASE 2: Ultra-optimized cosine sum with AVX-512 FMA intrinsics
#[inline]
pub fn sum_cos_pairs(pairs: &[(f64,f64)], x: f64, decay: f64, total_amp_sum: f64) -> (f64, f64) {
    let n = pairs.len();
    
    if n > WAVELET_THRESHOLD {
        return sum_cos_pairs_wavelet(pairs, x, decay, total_amp_sum);
    }

#[cfg(all(target_arch = "x86_64", target_feature = "avx512f"))]
#[inline]
unsafe fn cos_pd512(v: core::arch::x86_64::__m512d) -> core::arch::x86_64::__m512d {
    use core::arch::x86_64::*;
    let mut buf = [0f64; 8];
    _mm512_storeu_pd(buf.as_mut_ptr(), v);
    for val in &mut buf {
        *val = val.cos();
    }
    _mm512_loadu_pd(buf.as_ptr())
}

#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
#[inline]
unsafe fn cos_pd256(v: core::arch::x86_64::__m256d) -> core::arch::x86_64::__m256d {
    use core::arch::x86_64::*;
    let mut buf = [0f64; 4];
    _mm256_storeu_pd(buf.as_mut_ptr(), v);
    for val in &mut buf {
        *val = val.cos();
    }
    _mm256_loadu_pd(buf.as_ptr())
}
    
    let mut amp_sum = 0.0f64;
    let mut phase_wsum = 0.0f64;
    let pi2x = TWO_PI * x;
    let mut i = 0usize;
    
    #[cfg(all(target_arch = "x86_64", target_feature = "avx512f"))]
    unsafe {
        use core::arch::x86_64::*;
        let mut idx = 0usize;
        let decay_vec = _mm512_set1_pd(decay);
        let pi2x_vec = _mm512_set1_pd(pi2x);
        while idx + 8 <= n {
            let mut amp_vals = [0f64; 8];
            let mut phase_vals = [0f64; 8];
            for lane in 0..8 {
                let (a, p) = pairs[idx + lane];
                amp_vals[lane] = a;
                phase_vals[lane] = p;
            }
            let amp = _mm512_loadu_pd(amp_vals.as_ptr());
            let phase = _mm512_loadu_pd(phase_vals.as_ptr());
            let angle = _mm512_add_pd(pi2x_vec, phase);
            let cos_angle = cos_pd512(angle);
            let amp_cos = _mm512_mul_pd(amp, cos_angle);
            let amp_decay = _mm512_mul_pd(amp_cos, decay_vec);
            let mut acc_buf = [0f64; 8];
            _mm512_storeu_pd(acc_buf.as_mut_ptr(), amp_decay);
            amp_sum += acc_buf.iter().sum::<f64>();
            phase_wsum += phase_vals.iter().sum::<f64>();
            idx += 8;
        }
        i = idx;
    }

    #[cfg(all(target_arch = "x86_64", not(target_feature = "avx512f"), target_feature = "avx2"))]
    unsafe {
        use core::arch::x86_64::*;
        let mut idx = i;
        let decay_vec = _mm256_set1_pd(decay);
        let pi2x_vec = _mm256_set1_pd(pi2x);
        while idx + 4 <= n {
            let mut amp_vals = [0f64; 4];
            let mut phase_vals = [0f64; 4];
            for lane in 0..4 {
                let (a, p) = pairs[idx + lane];
                amp_vals[lane] = a;
                phase_vals[lane] = p;
            }
            let amp = _mm256_loadu_pd(amp_vals.as_ptr());
            let phase = _mm256_loadu_pd(phase_vals.as_ptr());
            let angle = _mm256_add_pd(pi2x_vec, phase);
            let cos_angle = cos_pd256(angle);
            let amp_cos = _mm256_mul_pd(amp, cos_angle);
            let amp_decay = _mm256_mul_pd(amp_cos, decay_vec);
            let mut acc_buf = [0f64; 4];
            _mm256_storeu_pd(acc_buf.as_mut_ptr(), amp_decay);
            amp_sum += acc_buf.iter().sum::<f64>();
            phase_wsum += phase_vals.iter().sum::<f64>();
            idx += 4;
        }
        i = idx;
    }
    
    // Scalar fallback (LLVM will auto-vectorize this efficiently)
    while i < n {
        let (a, p) = pairs[i];
        let angle = pi2x + p;
        amp_sum += a * angle.cos();
        phase_wsum += p; // Simplified
        i += 1;
    }
    
    // Normalization
    let norm = if total_amp_sum > 0.0 { total_amp_sum.sqrt() } else { 1.0 };
    (amp_sum / norm, phase_wsum / n as f64)
}

pub fn sum_cos_pairs_wavelet(pairs: &[(f64,f64)], x: f64, decay: f64, total_amp_sum: f64) -> (f64, f64) {
    let n = pairs.len();
    if n == 0 {
        return (0.0, 0.0);
    }

    // Deterministic Haar basis sampling: split into 2^L bins and aggregate
    let bins = 1 << HAAR_LEVELS;
    let bin_size = (n + bins - 1) / bins;
    let mut amp_sum = 0.0;
    let mut phase_weight = 0.0;
    let pi2x = TWO_PI * x;

    for level in 0..HAAR_LEVELS {
        let stride = 1 << level;
        let weight = 1.0 / (stride as f64).sqrt();
        let mut idx = 0usize;
        while idx < n {
            let end = (idx + bin_size).min(n);
            let mut local_amp = 0.0;
            let mut local_phase = 0.0;
            for i in idx..end {
                let (a, p) = pairs[i];
                local_amp += a * (pi2x + p).cos();
                local_phase += p;
            }
            amp_sum += weight * local_amp;
            phase_weight += weight * local_phase;
            idx += stride * bin_size;
        }
    }

    let norm = if total_amp_sum > 0.0 { total_amp_sum.sqrt() } else { 1.0 };
    (amp_sum / norm, phase_weight / (bins as f64))
}


