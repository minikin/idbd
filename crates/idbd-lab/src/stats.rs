//! Summary statistics for repeated measurements.

/// Mean and uncertainty of a set of measurements, one per seed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Summary {
    count: usize,
    mean: f64,
    standard_error: f64,
}

impl Summary {
    /// Summarizes `values`, or returns `None` when there are none.
    ///
    /// The standard error is the Bessel-corrected sample standard deviation divided by
    /// the square root of the count. A single value reports zero, which records "no
    /// spread was measured", not "the mean is exact".
    pub fn new(values: &[f64]) -> Option<Self> {
        let count = values.len();
        if count == 0 {
            return None;
        }
        let n = exact_f64(count_to_u64(count));
        let mean = mean(values, n);
        let standard_error = if count < 2 {
            0.0
        } else {
            standard_error(values, mean, n)
        };
        Some(Self {
            count,
            mean,
            standard_error,
        })
    }

    /// Number of measurements.
    pub const fn count(self) -> usize {
        self.count
    }

    /// Arithmetic mean.
    pub const fn mean(self) -> f64 {
        self.mean
    }

    /// Standard error of the mean.
    pub const fn standard_error(self) -> f64 {
        self.standard_error
    }
}

/// Keeps ordinary measurements bitwise unchanged, scaling only if their sum overflows.
fn mean(values: &[f64], n: f64) -> f64 {
    let mean = values.iter().sum::<f64>() / n;
    if mean.is_finite() || !values.iter().all(|value| value.is_finite()) {
        return mean;
    }
    let scale = values.iter().map(|value| value.abs()).fold(0.0, f64::max);
    (values.iter().map(|value| value / scale).sum::<f64>() / n) * scale
}

/// Bessel-corrected standard error of at least two values. Deviations are divided by the
/// largest one before squaring, so a run whose error reached 1e186 still gets a finite
/// spread instead of an overflow.
fn standard_error(values: &[f64], mean: f64, n: f64) -> f64 {
    let scale = values.iter().map(|v| (v - mean).abs()).fold(0.0, f64::max);
    if scale.is_infinite() && mean.is_finite() && values.iter().all(|value| value.is_finite()) {
        // Opposite-sign values can overflow even the subtraction. Normalize before it.
        let scale = values.iter().map(|value| value.abs()).fold(0.0, f64::max);
        let normalized_mean = mean / scale;
        let sum_sq: f64 = values
            .iter()
            .map(|value| (value / scale - normalized_mean).powi(2))
            .sum();
        return scale * (sum_sq / (n - 1.0) / n).sqrt();
    }
    if scale == 0.0 || !scale.is_finite() {
        return scale;
    }
    let sum_sq: f64 = values.iter().map(|v| ((v - mean) / scale).powi(2)).sum();
    scale * (sum_sq / (n - 1.0) / n).sqrt()
}

/// Returns the nearest-rank percentile of an ascending slice, or `None` if it is empty.
///
/// `fraction` is clamped to `[0, 1]`. The rank is `round(fraction * (len - 1))`, so
/// `0.5` gives the lower of the two middle values for an even length.
///
/// # Panics
///
/// Panics if `sorted` is not in ascending order, since an unsorted input would silently
/// return a meaningless number.
pub fn percentile(sorted: &[f64], fraction: f64) -> Option<f64> {
    assert!(
        sorted.windows(2).all(|w| w[0] <= w[1]),
        "percentile needs an ascending slice"
    );
    let last = sorted.len().checked_sub(1)?;
    let rank = fraction.clamp(0.0, 1.0) * exact_f64(count_to_u64(last));
    let distance = |index: usize| (exact_f64(count_to_u64(index)) - rank).abs();
    let index = (0..=last)
        .min_by(|&a, &b| {
            distance(a)
                .partial_cmp(&distance(b))
                .expect("distances to a finite rank are comparable")
        })
        .expect("the inclusive range always contains index 0");
    sorted.get(index).copied()
}

/// Returns the median of an ascending slice.
///
/// # Panics
///
/// Panics if `sorted` is not in ascending order.
pub fn median(sorted: &[f64]) -> Option<f64> {
    percentile(sorted, 0.5)
}

/// Converts a count to `f64` without a lossy cast.
///
/// # Panics
///
/// Panics if `count` exceeds `u32::MAX`. No recorded run approaches that many samples.
pub fn exact_f64(count: u64) -> f64 {
    f64::from(u32::try_from(count).expect("recorded counts stay below 2^32"))
}

fn count_to_u64(count: usize) -> u64 {
    u64::try_from(count).expect("a slice length fits in 64 bits")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summaries_of_large_finite_values_stay_representable() {
        for values in [[1e308, 1e308], [-1e308, -1e308], [0.0, 0.0]] {
            let summary = Summary::new(&values).expect("non-empty");
            assert_eq!(summary.mean().to_bits(), values[0].to_bits(), "{values:?}");
            assert_eq!(
                summary.standard_error().to_bits(),
                0.0_f64.to_bits(),
                "{values:?}"
            );
        }
        let summary = Summary::new(&[-f64::MAX, f64::MAX]).expect("non-empty");
        assert_eq!(summary.mean().to_bits(), 0.0_f64.to_bits(), "{summary:?}");
        assert!(
            (summary.standard_error() / f64::MAX - 1.0).abs() < 1e-12,
            "{summary:?}"
        );
        let summary = Summary::new(&[-f64::MAX, f64::MAX, f64::MAX]).expect("non-empty");
        assert!(
            (summary.mean() / (f64::MAX / 3.0) - 1.0).abs() < 1e-12,
            "{summary:?}"
        );
        assert!(
            (summary.standard_error() / (f64::MAX * (2.0 / 3.0)) - 1.0).abs() < 1e-12,
            "{summary:?}"
        );
    }

    #[test]
    fn standard_error_stays_finite_when_squared_deviations_would_overflow() {
        let s = Summary::new(&[1e186, 3e186]).expect("non-empty");
        assert!((s.standard_error() / 1e186 - 1.0).abs() < 1e-12, "{s:?}");
        let near_max = Summary::new(&[0.0, 1.5e308]).expect("non-empty");
        assert!(
            (near_max.standard_error() / 0.75e308 - 1.0).abs() < 1e-12,
            "{near_max:?}"
        );
        let infinite = Summary::new(&[1.0, f64::INFINITY]).expect("non-empty");
        assert!(!infinite.standard_error().is_finite(), "{infinite:?}");
    }

    #[test]
    fn summary_reports_mean_and_standard_error() {
        let s = Summary::new(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]).expect("non-empty");
        assert_eq!(s.count(), 8);
        assert!((s.mean() - 5.0).abs() < 1e-12, "{s:?}");
        // Sample standard deviation is sqrt(32/7) = 2.13809...; divide by sqrt(8).
        let expected = (32.0_f64 / 7.0 / 8.0).sqrt();
        assert!(
            (s.standard_error() - expected).abs() < 1e-12,
            "{} vs {expected}",
            s.standard_error()
        );
    }

    #[test]
    fn summary_of_one_value_has_no_spread() {
        let s = Summary::new(&[3.25]).expect("non-empty");
        assert_eq!(s.count(), 1);
        assert!((s.mean() - 3.25).abs() < 1e-12, "{s:?}");
        assert!(s.standard_error().abs() < 1e-12, "{s:?}");
    }

    #[test]
    fn summary_of_nothing_is_none() {
        assert_eq!(Summary::new(&[]), None);
    }

    #[test]
    fn identical_values_have_zero_standard_error() {
        let s = Summary::new(&[1.5; 6]).expect("non-empty");
        assert!(s.standard_error().abs() < 1e-12, "{s:?}");
    }

    #[test]
    fn percentiles_use_nearest_rank() {
        let sorted = [1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(percentile(&sorted, 0.0), Some(1.0));
        assert_eq!(percentile(&sorted, 0.5), Some(3.0));
        assert_eq!(percentile(&sorted, 1.0), Some(5.0));
        assert_eq!(percentile(&sorted, 0.1), Some(1.0));
        assert_eq!(percentile(&sorted, 0.9), Some(5.0));
        assert_eq!(median(&sorted), Some(3.0));
        assert_eq!(percentile(&[], 0.5), None);
        assert_eq!(median(&[7.0, 8.0]), Some(7.0));
    }

    #[test]
    fn percentile_fractions_are_clamped() {
        let sorted = [1.0, 2.0];
        assert_eq!(percentile(&sorted, -5.0), Some(1.0));
        assert_eq!(percentile(&sorted, 5.0), Some(2.0));
    }

    #[test]
    #[should_panic(expected = "ascending slice")]
    fn percentile_rejects_unsorted_input() {
        let _ = percentile(&[2.0, 1.0], 0.5);
    }

    #[test]
    fn exact_f64_converts_counts_without_loss() {
        for count in [0_u64, 1, 2, 999, 250_000, 4_294_967_295] {
            let converted = exact_f64(count);
            assert!(
                converted >= 0.0 && converted.fract() < f64::EPSILON,
                "count {count} became {converted}"
            );
            assert!(converted + 1.0 > converted, "count {count} saturated");
        }
    }
}
