use std::{cmp::min, f64::consts::{E, LN_2, LOG2_E}};


pub struct ExponentialHistogram {
    scale: u32,
    max_bucket_index: u32,
    positive_buckets: Vec<usize>,
    negative_buckets: Vec<usize>,
}

impl ExponentialHistogram {
    pub fn new(scale: u32) -> Self {
        Self::new_with_max_buckets(scale, 160)
    }

    pub fn new_with_max_buckets(scale: u32, max_buckets: u32) -> Self {
        Self {
            scale,
            max_bucket_index: max_buckets - 1,
            positive_buckets: Default::default(),
            negative_buckets: Default::default(),
        }
    }

    pub fn accumulate<T: Into<f64>>(&mut self, value: T) {
        let value = value.into();
        let index = self.map_to_index(value);
        let buckets = if value.is_sign_positive() {
            &mut self.positive_buckets
        } else {
            &mut self.negative_buckets
        };
        if buckets.is_empty() {
            // I could reserve these ahead of time, but it seems likely that many uses will have exclusively
            // positive or exclusively negative numbers - so this saves memory in those cases.
            buckets.reserve_exact(self.max_bucket_index as usize + 1);
            // SAFETY: immediately initialize to 0 without doing repeated capacity checks, only index checks
            unsafe { buckets.set_len(self.max_bucket_index as usize + 1) };
            for i in &mut *buckets { // reborrow
                *i = 0;
            }
        }
        buckets[index] += 1;
    }

    /// treats negative numbers as positive - you gotta accumulate into a negative array
    fn map_to_index(&self, raw_value: f64) -> usize {
        let value = raw_value.abs();
        let scale_factor = LOG2_E * 2_f64.powi(self.scale as i32);
        let index = (value.log(E) * scale_factor).floor() as usize;
        min(self.max_bucket_index as usize, index)
    }

    /// obviously only supports positive indices. If you want a negative boundary, flip the sign on the return value.
    /// per the wonkadoo instructions found at: https://opentelemetry.io/docs/specs/otel/metrics/data-model/#exponentialhistogram
    ///   > The positive and negative ranges of the histogram are expressed separately. Negative values are mapped by
    ///   > their absolute value into the negative range using the same scale as the positive range. Note that in the
    ///   > negative range, therefore, histogram buckets use lower-inclusive boundaries.
    fn lower_boundary(&self, index: usize) -> f64 {
        let index = min(self.max_bucket_index as usize, index);
        let inverse_scale_factor = LN_2 * 2_f64.powi(-(self.scale as i32));
        (index as f64 * inverse_scale_factor).exp()
    }
}

#[cfg(test)]
mod test {
    use super::ExponentialHistogram;

    #[test]
    fn indices_scale_zero_positive_numbers() {
        let e = ExponentialHistogram::new(0);

        assert_eq!(0, e.map_to_index(0_f64));
        assert_value_index_lowerboundary(&e, 0, 1);
        assert_value_index_lowerboundary(&e, 1, 1);
        assert_value_index_lowerboundary(&e, 2, 2);
        assert_value_index_lowerboundary(&e, 3, 2);
        assert_value_index_lowerboundary(&e, 4, 4);
        assert_value_index_lowerboundary(&e, 7, 4);
        assert_value_index_lowerboundary(&e, 8, 4);
        assert_value_index_lowerboundary(&e, 8.1, 8);
    }

    #[test]
    fn indices_scale_zero_negative_numbers() {
        let e = ExponentialHistogram::new(0);

        assert_eq!(0, e.map_to_index(0_f64));
        assert_value_index_lowerboundary(&e, -0, 1);
        assert_value_index_lowerboundary(&e, -1, 1);
        assert_value_index_lowerboundary(&e, -2, 2);
        assert_value_index_lowerboundary(&e, -3, 2);
        assert_value_index_lowerboundary(&e, -4, 4);
        assert_value_index_lowerboundary(&e, -7, 4);
        assert_value_index_lowerboundary(&e, -8, 4);
        assert_value_index_lowerboundary(&e, -8.1, 8);
    }

    #[test]
    fn indices_scale_one_positive_numbers() {
        let e = ExponentialHistogram::new(1);

        assert_eq!(0, e.map_to_index(0_f64));
        assert_value_index_lowerboundary(&e, 0, 1);
        assert_value_index_lowerboundary(&e, 1, 1);
        assert_value_index_lowerboundary(&e, 2, 2);
        assert_value_index_lowerboundary(&e, 3, 2.828);
        assert_value_index_lowerboundary(&e, 4, 4);
        assert_value_index_lowerboundary(&e, 7, 5.657);
        assert_value_index_lowerboundary(&e, 8, 5.657);
        assert_value_index_lowerboundary(&e, 8.1, 8);

        assert_eq_epsilon(854839645001008300000000_f64, e.lower_boundary(160), "scale 1 goes very high with 160 buckets");
    }

    #[test]
    fn indices_scale_two_positive_numbers() {
        let e = ExponentialHistogram::new(2);

        assert_eq!(0, e.map_to_index(0_f64));
        assert_value_index_lowerboundary(&e, 0, 1);
        assert_value_index_lowerboundary(&e, 1, 1);
        assert_value_index_lowerboundary(&e, 2, 2);
        assert_value_index_lowerboundary(&e, 3, 2.828);
        assert_value_index_lowerboundary(&e, 4, 4);
        assert_value_index_lowerboundary(&e, 7, 6.727);
        assert_value_index_lowerboundary(&e, 8, 6.727);
        assert_value_index_lowerboundary(&e, 8.1, 8);

        assert_eq_epsilon(924575386326.615_f64, e.lower_boundary(159), "scale 2 goes to 924 billion with 160 buckets");
        assert_eq_epsilon(777472127993.868_f64, e.lower_boundary(158), "scale 2 loses a lot of precision at the end of the scale");
        
        assert_eq_epsilon(28215801.585_f64, e.lower_boundary(99), "scale 2 has a reasonable precision in the middle");
        assert_eq_epsilon(33554432.000_f64, e.lower_boundary(100), "scale 2 has a reasonable precision in the middle");
        assert_eq_epsilon(39903169.274_f64, e.lower_boundary(101), "scale 2 has a reasonable precision in the middle");

        assert_eq_epsilon(881743.800_f64, e.lower_boundary(79), "scale 2 has a reasonable precision near 1 million");
        assert_eq_epsilon(1048576.000_f64, e.lower_boundary(80), "scale 2 has a reasonable precision near 1 million");
        assert_eq_epsilon(1246974.040_f64, e.lower_boundary(81), "scale 2 has a reasonable precision near 1 million");
    }

    #[test]
    fn indices_scale_three_positive_numbers() {
        let e = ExponentialHistogram::new(3);

        assert_eq!(0, e.map_to_index(0_f64));
        assert_value_index_lowerboundary(&e, 0, 1);
        assert_value_index_lowerboundary(&e, 1, 1);
        assert_value_index_lowerboundary(&e, 2, 2);
        assert_value_index_lowerboundary(&e, 3, 2.828);
        assert_value_index_lowerboundary(&e, 4, 4);
        assert_value_index_lowerboundary(&e, 7, 6.727);
        assert_value_index_lowerboundary(&e, 8, 7.337);
        assert_value_index_lowerboundary(&e, 8.1, 8);

        assert_eq_epsilon(961548.432_f64, e.lower_boundary(160), "scale 3 goes to just 0.96 million with 160 buckets");
    }

    #[test]
    fn indices_scale_four_positive_numbers() {
        let e = ExponentialHistogram::new(4);

        assert_eq!(0, e.map_to_index(0_f64));
        assert_value_index_lowerboundary(&e, 0, 1);
        assert_value_index_lowerboundary(&e, 1, 1);
        assert_value_index_lowerboundary(&e, 2, 2);
        assert_value_index_lowerboundary(&e, 3, 2.954);
        assert_value_index_lowerboundary(&e, 4, 4);
        assert_value_index_lowerboundary(&e, 5, 4.967);
        assert_value_index_lowerboundary(&e, 6, 5.907);
        assert_value_index_lowerboundary(&e, 7, 6.727);
        assert_value_index_lowerboundary(&e, 8, 7.661);
        assert_value_index_lowerboundary(&e, 8.1, 8);

        assert_eq_epsilon(980.586_f64, e.lower_boundary(160), "scale 4 goes to just 980 with 160 buckets");
    }

    #[track_caller]
    fn assert_value_index_lowerboundary(e: &ExponentialHistogram, value: impl Into<f64>, expected_lower_boundary: impl Into<f64>) {
        let observed_index = e.map_to_index(value.into());
        let observed_boundary = e.lower_boundary(observed_index);
        assert_eq_epsilon(expected_lower_boundary.into(), observed_boundary, "boundary matches");
    }

    #[track_caller]
    fn assert_eq_epsilon(j: f64, k: f64, message: &str) {
        const EPSILON: f64 = 1.0 / 128.0;
        let difference = (j - k).abs();
        assert!(difference < EPSILON, "{message}: {j} != {k} with epsilon {EPSILON}.");
    }
}
