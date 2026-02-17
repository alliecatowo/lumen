//! SIMD-accelerated operations for hot loops.
//!
//! Uses manual loop unrolling (4×f64 per iteration) to enable auto-vectorization
//! on stable Rust. The compiler will typically emit AVX/SSE instructions for these
//! patterns when building with `-C target-cpu=native` or on x86_64 targets.

/// SIMD-accelerated dot product of two equal-length slices.
///
/// Processes 4 elements at a time with manual unrolling, then handles the
/// remainder. Equivalent to `a.iter().zip(b).map(|(x,y)| x*y).sum()`.
///
/// # Panics
///
/// Panics if `a` and `b` have different lengths.
#[inline]
pub fn simd_dot_product(a: &[f64], b: &[f64]) -> f64 {
    assert_eq!(a.len(), b.len(), "simd_dot_product: length mismatch");
    let n = a.len();
    let chunks = n / 4;
    let remainder = n % 4;

    // Four accumulators to break dependency chains and enable pipelining.
    let mut acc0: f64 = 0.0;
    let mut acc1: f64 = 0.0;
    let mut acc2: f64 = 0.0;
    let mut acc3: f64 = 0.0;

    let base = 0;
    for i in 0..chunks {
        let offset = base + i * 4;
        // SAFETY: bounds are guaranteed by chunks calculation.
        unsafe {
            acc0 += *a.get_unchecked(offset) * *b.get_unchecked(offset);
            acc1 += *a.get_unchecked(offset + 1) * *b.get_unchecked(offset + 1);
            acc2 += *a.get_unchecked(offset + 2) * *b.get_unchecked(offset + 2);
            acc3 += *a.get_unchecked(offset + 3) * *b.get_unchecked(offset + 3);
        }
    }

    // Handle remainder elements.
    let tail_start = chunks * 4;
    for i in 0..remainder {
        unsafe {
            acc0 += *a.get_unchecked(tail_start + i) * *b.get_unchecked(tail_start + i);
        }
    }

    acc0 + acc1 + acc2 + acc3
}

/// SIMD-accelerated element-wise addition: `out[i] = a[i] + b[i]`.
///
/// Processes 4 elements at a time with manual unrolling.
///
/// # Panics
///
/// Panics if `a`, `b`, and `out` do not all have the same length.
#[inline]
pub fn simd_add(a: &[f64], b: &[f64], out: &mut [f64]) {
    let n = a.len();
    assert_eq!(n, b.len(), "simd_add: a and b length mismatch");
    assert_eq!(n, out.len(), "simd_add: output length mismatch");

    let chunks = n / 4;
    let remainder = n % 4;

    for i in 0..chunks {
        let offset = i * 4;
        unsafe {
            *out.get_unchecked_mut(offset) = *a.get_unchecked(offset) + *b.get_unchecked(offset);
            *out.get_unchecked_mut(offset + 1) =
                *a.get_unchecked(offset + 1) + *b.get_unchecked(offset + 1);
            *out.get_unchecked_mut(offset + 2) =
                *a.get_unchecked(offset + 2) + *b.get_unchecked(offset + 2);
            *out.get_unchecked_mut(offset + 3) =
                *a.get_unchecked(offset + 3) + *b.get_unchecked(offset + 3);
        }
    }

    let tail_start = chunks * 4;
    for i in 0..remainder {
        unsafe {
            *out.get_unchecked_mut(tail_start + i) =
                *a.get_unchecked(tail_start + i) + *b.get_unchecked(tail_start + i);
        }
    }
}

/// SIMD-accelerated element-wise multiplication: `out[i] = a[i] * b[i]`.
///
/// Processes 4 elements at a time with manual unrolling.
///
/// # Panics
///
/// Panics if `a`, `b`, and `out` do not all have the same length.
#[inline]
pub fn simd_mul(a: &[f64], b: &[f64], out: &mut [f64]) {
    let n = a.len();
    assert_eq!(n, b.len(), "simd_mul: a and b length mismatch");
    assert_eq!(n, out.len(), "simd_mul: output length mismatch");

    let chunks = n / 4;
    let remainder = n % 4;

    for i in 0..chunks {
        let offset = i * 4;
        unsafe {
            *out.get_unchecked_mut(offset) = *a.get_unchecked(offset) * *b.get_unchecked(offset);
            *out.get_unchecked_mut(offset + 1) =
                *a.get_unchecked(offset + 1) * *b.get_unchecked(offset + 1);
            *out.get_unchecked_mut(offset + 2) =
                *a.get_unchecked(offset + 2) * *b.get_unchecked(offset + 2);
            *out.get_unchecked_mut(offset + 3) =
                *a.get_unchecked(offset + 3) * *b.get_unchecked(offset + 3);
        }
    }

    let tail_start = chunks * 4;
    for i in 0..remainder {
        unsafe {
            *out.get_unchecked_mut(tail_start + i) =
                *a.get_unchecked(tail_start + i) * *b.get_unchecked(tail_start + i);
        }
    }
}

/// SIMD-accelerated sum reduction.
///
/// Processes 4 elements at a time with four accumulators to break dependency
/// chains.
#[inline]
pub fn simd_sum(a: &[f64]) -> f64 {
    let n = a.len();
    let chunks = n / 4;
    let remainder = n % 4;

    let mut acc0: f64 = 0.0;
    let mut acc1: f64 = 0.0;
    let mut acc2: f64 = 0.0;
    let mut acc3: f64 = 0.0;

    for i in 0..chunks {
        let offset = i * 4;
        unsafe {
            acc0 += *a.get_unchecked(offset);
            acc1 += *a.get_unchecked(offset + 1);
            acc2 += *a.get_unchecked(offset + 2);
            acc3 += *a.get_unchecked(offset + 3);
        }
    }

    let tail_start = chunks * 4;
    for i in 0..remainder {
        unsafe {
            acc0 += *a.get_unchecked(tail_start + i);
        }
    }

    acc0 + acc1 + acc2 + acc3
}

/// SIMD-accelerated scalar multiply: `out[i] = a[i] * scalar`.
///
/// Processes 4 elements at a time with manual unrolling.
///
/// # Panics
///
/// Panics if `a` and `out` have different lengths.
#[inline]
pub fn simd_scale(a: &[f64], scalar: f64, out: &mut [f64]) {
    let n = a.len();
    assert_eq!(n, out.len(), "simd_scale: output length mismatch");

    let chunks = n / 4;
    let remainder = n % 4;

    for i in 0..chunks {
        let offset = i * 4;
        unsafe {
            *out.get_unchecked_mut(offset) = *a.get_unchecked(offset) * scalar;
            *out.get_unchecked_mut(offset + 1) = *a.get_unchecked(offset + 1) * scalar;
            *out.get_unchecked_mut(offset + 2) = *a.get_unchecked(offset + 2) * scalar;
            *out.get_unchecked_mut(offset + 3) = *a.get_unchecked(offset + 3) * scalar;
        }
    }

    let tail_start = chunks * 4;
    for i in 0..remainder {
        unsafe {
            *out.get_unchecked_mut(tail_start + i) = *a.get_unchecked(tail_start + i) * scalar;
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-10;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS
    }

    // ── simd_dot_product ────────────────────────────────────────────────

    #[test]
    fn dot_product_basic() {
        let a = [1.0, 2.0, 3.0, 4.0];
        let b = [5.0, 6.0, 7.0, 8.0];
        let result = simd_dot_product(&a, &b);
        // 1*5 + 2*6 + 3*7 + 4*8 = 5 + 12 + 21 + 32 = 70
        assert!(approx_eq(result, 70.0));
    }

    #[test]
    fn dot_product_with_remainder() {
        // 7 elements: 4 in chunk + 3 remainder
        let a = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let b = [2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0];
        let result = simd_dot_product(&a, &b);
        // (1+2+3+4+5+6+7) * 2 = 56
        assert!(approx_eq(result, 56.0));
    }

    #[test]
    fn dot_product_empty() {
        let a: [f64; 0] = [];
        let b: [f64; 0] = [];
        assert!(approx_eq(simd_dot_product(&a, &b), 0.0));
    }

    #[test]
    fn dot_product_single() {
        let a = [3.0];
        let b = [4.0];
        assert!(approx_eq(simd_dot_product(&a, &b), 12.0));
    }

    #[test]
    fn dot_product_large() {
        let n = 1024;
        let a: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let b: Vec<f64> = vec![1.0; n];
        let result = simd_dot_product(&a, &b);
        let expected: f64 = (0..n).map(|i| i as f64).sum();
        assert!(approx_eq(result, expected));
    }

    // ── simd_add ────────────────────────────────────────────────────────

    #[test]
    fn add_basic() {
        let a = [1.0, 2.0, 3.0, 4.0];
        let b = [5.0, 6.0, 7.0, 8.0];
        let mut out = [0.0; 4];
        simd_add(&a, &b, &mut out);
        assert!(approx_eq(out[0], 6.0));
        assert!(approx_eq(out[1], 8.0));
        assert!(approx_eq(out[2], 10.0));
        assert!(approx_eq(out[3], 12.0));
    }

    #[test]
    fn add_with_remainder() {
        let a = [1.0, 2.0, 3.0, 4.0, 5.0];
        let b = [10.0, 20.0, 30.0, 40.0, 50.0];
        let mut out = [0.0; 5];
        simd_add(&a, &b, &mut out);
        assert!(approx_eq(out[4], 55.0));
    }

    // ── simd_mul ────────────────────────────────────────────────────────

    #[test]
    fn mul_basic() {
        let a = [2.0, 3.0, 4.0, 5.0];
        let b = [1.5, 2.5, 3.5, 4.5];
        let mut out = [0.0; 4];
        simd_mul(&a, &b, &mut out);
        assert!(approx_eq(out[0], 3.0));
        assert!(approx_eq(out[1], 7.5));
        assert!(approx_eq(out[2], 14.0));
        assert!(approx_eq(out[3], 22.5));
    }

    #[test]
    fn mul_with_remainder() {
        let a = [1.0, 2.0, 3.0];
        let b = [4.0, 5.0, 6.0];
        let mut out = [0.0; 3];
        simd_mul(&a, &b, &mut out);
        assert!(approx_eq(out[0], 4.0));
        assert!(approx_eq(out[1], 10.0));
        assert!(approx_eq(out[2], 18.0));
    }

    // ── simd_sum ────────────────────────────────────────────────────────

    #[test]
    fn sum_basic() {
        let a = [1.0, 2.0, 3.0, 4.0];
        assert!(approx_eq(simd_sum(&a), 10.0));
    }

    #[test]
    fn sum_with_remainder() {
        let a = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        assert!(approx_eq(simd_sum(&a), 28.0));
    }

    #[test]
    fn sum_empty() {
        let a: [f64; 0] = [];
        assert!(approx_eq(simd_sum(&a), 0.0));
    }

    #[test]
    fn sum_single() {
        let a = [42.0];
        assert!(approx_eq(simd_sum(&a), 42.0));
    }

    // ── simd_scale ──────────────────────────────────────────────────────

    #[test]
    fn scale_basic() {
        let a = [1.0, 2.0, 3.0, 4.0];
        let mut out = [0.0; 4];
        simd_scale(&a, 3.0, &mut out);
        assert!(approx_eq(out[0], 3.0));
        assert!(approx_eq(out[1], 6.0));
        assert!(approx_eq(out[2], 9.0));
        assert!(approx_eq(out[3], 12.0));
    }

    #[test]
    fn scale_with_remainder() {
        let a = [1.0, 2.0, 3.0, 4.0, 5.0];
        let mut out = [0.0; 5];
        simd_scale(&a, -2.0, &mut out);
        assert!(approx_eq(out[0], -2.0));
        assert!(approx_eq(out[4], -10.0));
    }

    #[test]
    fn scale_zero() {
        let a = [1.0, 2.0, 3.0];
        let mut out = [0.0; 3];
        simd_scale(&a, 0.0, &mut out);
        assert!(approx_eq(out[0], 0.0));
        assert!(approx_eq(out[1], 0.0));
        assert!(approx_eq(out[2], 0.0));
    }

    // ── Cross-validation with naive ─────────────────────────────────────

    #[test]
    fn dot_product_matches_naive() {
        let a: Vec<f64> = (0..33).map(|i| (i as f64) * 0.7).collect();
        let b: Vec<f64> = (0..33).map(|i| (i as f64) * 1.3).collect();

        let naive: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let simd = simd_dot_product(&a, &b);
        assert!((naive - simd).abs() < 1e-6);
    }

    #[test]
    fn sum_matches_naive() {
        let a: Vec<f64> = (0..129).map(|i| (i as f64) * 0.3).collect();
        let naive: f64 = a.iter().sum();
        let simd = simd_sum(&a);
        assert!((naive - simd).abs() < 1e-6);
    }
}
