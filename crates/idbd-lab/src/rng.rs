//! A small, versioned, seedable random generator.

/// Identifies the generator and the normal transform, so recorded results name exactly
/// how their samples were drawn.
pub const GENERATOR: &str = "xoshiro256** seeded by SplitMix64, Box-Muller normals (v1)";

/// `xoshiro256**` (Blackman and Vigna, 2018) with a Box-Muller normal transform.
///
/// The output for a given seed is part of the reproducibility contract: changing it
/// requires a new [`GENERATOR`] version.
#[derive(Debug, Clone)]
pub struct Rng {
    state: [u64; 4],
    spare_normal: Option<f64>,
}

impl Rng {
    /// Creates a generator whose state is expanded from `seed` by `SplitMix64`.
    pub fn new(seed: u64) -> Self {
        let mut sm = seed;
        let mut next = || {
            sm = sm.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = sm;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        Self {
            state: [next(), next(), next(), next()],
            spare_normal: None,
        }
    }

    /// Returns the next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        let [s0, s1, s2, s3] = &mut self.state;
        let result = s1.wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = *s1 << 17;
        *s2 ^= *s0;
        *s3 ^= *s1;
        *s1 ^= *s2;
        *s0 ^= *s3;
        *s2 ^= t;
        *s3 = s3.rotate_left(45);
        result
    }

    /// Returns a uniform value in `[0, 1)` with 52 random mantissa bits.
    pub fn uniform(&mut self) -> f64 {
        f64::from_bits((0x3FF << 52) | (self.next_u64() >> 12)) - 1.0
    }

    /// Returns a uniform integer in `0..bound`.
    ///
    /// # Panics
    ///
    /// Panics if `bound` is zero.
    pub fn below(&mut self, bound: u64) -> u64 {
        assert!(bound > 0, "bound must be positive");
        // Lemire's multiply-shift; the bias is below 2^-58 for the small bounds used here.
        let wide = (u128::from(self.next_u64()) * u128::from(bound)) >> 64;
        u64::try_from(wide).expect("the high half of a 128-bit product fits in 64 bits")
    }

    /// Returns a uniform index in `0..len`.
    ///
    /// # Panics
    ///
    /// Panics if `len` is zero.
    pub fn index(&mut self, len: usize) -> usize {
        let bound = u64::try_from(len).expect("lengths fit in 64 bits");
        usize::try_from(self.below(bound)).expect("an index below a usize length fits")
    }

    /// Returns a standard normal value.
    pub fn standard_normal(&mut self) -> f64 {
        if let Some(value) = self.spare_normal.take() {
            return value;
        }
        let radius = (-2.0 * (1.0 - self.uniform()).ln()).sqrt();
        let angle = std::f64::consts::TAU * self.uniform();
        self.spare_normal = Some(radius * angle.sin());
        radius * angle.cos()
    }

    /// Returns `1.0` or `-1.0` with equal probability.
    pub fn sign(&mut self) -> f64 {
        if self.next_u64() >> 63 == 0 {
            1.0
        } else {
            -1.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64(), "identical seeds");
        }
        assert_ne!(
            Rng::new(7).next_u64(),
            Rng::new(8).next_u64(),
            "distinct seeds"
        );
    }

    #[test]
    fn known_first_outputs_pin_the_stream() {
        // Changing these values changes every recorded experiment; bump GENERATOR.
        let mut rng = Rng::new(0);
        let first: Vec<u64> = (0..3).map(|_| rng.next_u64()).collect();
        assert_eq!(first, PINNED_SEED_ZERO, "seed 0 stream");
    }

    /// Computed independently with a Python transcription of the reference C code.
    const PINNED_SEED_ZERO: [u64; 3] = [
        11_091_344_671_253_066_420,
        13_793_997_310_169_335_082,
        1_900_383_378_846_508_768,
    ];

    #[test]
    fn uniform_and_below_stay_in_range() {
        let mut rng = Rng::new(3);
        for _ in 0..10_000 {
            let u = rng.uniform();
            assert!((0.0..1.0).contains(&u), "uniform {u}");
            assert!(rng.below(5) < 5, "below");
        }
    }

    #[test]
    fn normals_have_unit_moments() {
        let mut rng = Rng::new(11);
        let n = 200_000_u32;
        let (mut sum, mut sum_sq) = (0.0, 0.0);
        for _ in 0..n {
            let z = rng.standard_normal();
            sum += z;
            sum_sq += z * z;
        }
        let mean = sum / f64::from(n);
        let variance = sum_sq / f64::from(n) - mean * mean;
        assert!(mean.abs() < 0.01, "mean {mean}");
        assert!((variance - 1.0).abs() < 0.02, "variance {variance}");
    }

    #[test]
    fn sign_produces_both_values() {
        let mut rng = Rng::new(5);
        let positives = (0..1000).filter(|_| rng.sign() > 0.0).count();
        assert!((400..600).contains(&positives), "positives {positives}");
    }
}
