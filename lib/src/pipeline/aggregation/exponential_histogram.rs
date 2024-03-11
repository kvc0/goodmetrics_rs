use std::{
    cmp::min,
    collections::VecDeque,
    f64::consts::{E, LN_2, LOG2_E},
    sync::atomic::Ordering,
};

use crate::{pipeline::AbsorbDistribution, types::Distribution};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExponentialHistogram {
    desired_scale: u8,
    actual_scale: u8,
    max_bucket_count: u16,
    bucket_start_offset: u32,
    positive_buckets: VecDeque<usize>,
    negative_buckets: VecDeque<usize>,
}

impl ExponentialHistogram {
    /// Desired scale will drop as necessary to match the static max buckets configuration.
    /// This will happen dynamically in response to observed range. If your distribution
    /// range falls within 160 contiguous buckets somewhere the desired scale's range, then
    /// your output scale will match your desired scale. If your observed range exceeds 160
    /// buckets then scale will be reduced to reflect the data's width.
    pub fn new(desired_scale: u8) -> Self {
        Self::new_with_max_buckets(desired_scale, 160)
    }

    /// Desired scale will drop as necessary to match the static max buckets configuration.
    /// This will happen dynamically in response to observed range. If your distribution
    /// range falls within max_buckets contiguous buckets somewhere the desired scale's range,
    /// then your output scale will match your desired scale. If your observed range exceeds
    /// max_buckets then scale will be reduced to reflect the data's width.
    pub fn new_with_max_buckets(desired_scale: u8, max_buckets: u16) -> Self {
        Self {
            desired_scale,
            actual_scale: desired_scale,
            max_bucket_count: max_buckets,
            bucket_start_offset: 0,
            positive_buckets: Default::default(),
            negative_buckets: Default::default(),
        }
    }

    pub fn accumulate<T: Into<f64>>(&mut self, value: T) {
        self.accumulate_count(value, 1)
    }

    fn accumulate_count<T: Into<f64>>(&mut self, value: T, count: usize) {
        let value = value.into();

        // This may be before or after the current range, and that range might need to be expanded.
        let scale_index = map_value_to_scale_index(self.actual_scale, value);

        // Initialize the histogram to center on the first data point. That should probabilistically
        // reduce the amount of shifting we do over time, for normal distributions.
        if self.is_empty() {
            self.bucket_start_offset =
                (scale_index as u32).saturating_sub(self.max_bucket_count as u32 / 2);
        }
        let mut local_index = scale_index as i64 - self.bucket_start_offset as i64;

        while local_index < 0 && self.rotate_range_down_one_index() {
            local_index += 1
        }
        while self.max_bucket_count as i64 <= local_index && self.rotate_range_up_one_index() {
            local_index -= 1
        }
        if local_index < 0 || self.max_bucket_count as i64 <= local_index {
            if self.zoom_out() {
                self.accumulate(value);
                return;
            }
            // if we didn't zoom out then we're at the end of the range.
            local_index = self.max_bucket_count as i64 - 1;
        }

        let index = min(self.max_bucket_count as usize - 1, local_index as usize);
        let buckets = self.get_mut_buckets_for_value(value);
        buckets.extend((0..=local_index.saturating_sub(buckets.len() as i64)).map(|_| 0));

        buckets[index] += count;
    }

    /// Reset the aggregation to an empty initial state
    pub fn zero(&mut self) {
        self.actual_scale = self.desired_scale;
        self.bucket_start_offset = 0;
        self.positive_buckets.clear();
        self.negative_buckets.clear();
    }

    fn zoom_out(&mut self) -> bool {
        if self.actual_scale == 0 {
            return false;
        }
        let old_scale: i32 = self.actual_scale.into();
        let old_bucket_start_offset = self.bucket_start_offset as usize;
        let old_positives = self.take_positives();
        let old_negatives = self.take_negatives();

        self.actual_scale -= 1;
        self.bucket_start_offset = 0;

        // now just reingest
        for (old_index, count) in old_positives.into_iter().enumerate() {
            if 0 < count {
                let value = lower_boundary(old_scale, old_bucket_start_offset + old_index);
                self.accumulate_count(value, count)
            }
        }
        for (old_index, count) in old_negatives.into_iter().enumerate() {
            if 0 < count {
                let value = -lower_boundary(old_scale, old_bucket_start_offset + old_index);
                self.accumulate_count(value, count)
            }
        }

        true
    }

    pub fn is_empty(&self) -> bool {
        self.positive_buckets.is_empty() && self.negative_buckets.is_empty()
    }

    fn rotate_range_down_one_index(&mut self) -> bool {
        if self.positive_buckets.len() < self.max_bucket_count as usize
            && self.negative_buckets.len() < self.max_bucket_count as usize
        {
            if !self.positive_buckets.is_empty() {
                self.positive_buckets.push_front(0);
            }
            if !self.negative_buckets.is_empty() {
                self.negative_buckets.push_front(0);
            }
            self.bucket_start_offset -= 1;
            true
        } else {
            false
        }
    }

    fn rotate_range_up_one_index(&mut self) -> bool {
        if self.positive_buckets.front().copied().unwrap_or_default() == 0
            && self.negative_buckets.front().copied().unwrap_or_default() == 0
        {
            self.positive_buckets.pop_front();
            self.negative_buckets.pop_front();
            self.bucket_start_offset += 1;
            true
        } else {
            false
        }
    }

    pub fn count(&self) -> usize {
        let mut count = 0;
        for i in &self.positive_buckets {
            count += *i;
        }
        for i in &self.negative_buckets {
            count += *i;
        }
        count
    }

    /// This is an approximation, just using the positive buckets for the sum.
    pub fn sum(&self) -> f64 {
        self.positive_buckets
            .iter()
            .enumerate()
            .map(|(index, count)| lower_boundary(self.actual_scale, index) * *count as f64)
            .sum()
    }

    /// This is an approximation, just using the positive buckets for the min.
    pub fn min(&self) -> f64 {
        self.positive_buckets
            .iter()
            .enumerate()
            .filter(|(_, count)| 0 < **count)
            .map(|(index, _count)| lower_boundary(self.actual_scale, index))
            .next()
            .unwrap_or_default()
    }

    /// This is an approximation, just using the positive buckets for the max.
    pub fn max(&self) -> f64 {
        self.positive_buckets
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, count)| 0 < **count)
            .map(|(index, _count)| lower_boundary(self.actual_scale, index))
            .next()
            .unwrap_or_default()
    }

    pub fn scale(&self) -> u8 {
        self.actual_scale
    }

    pub fn bucket_start_offset(&self) -> usize {
        self.bucket_start_offset as usize
    }

    pub fn take_positives(&mut self) -> VecDeque<usize> {
        std::mem::take(&mut self.positive_buckets)
    }

    pub fn take_negatives(&mut self) -> VecDeque<usize> {
        std::mem::take(&mut self.negative_buckets)
    }

    pub fn has_negatives(&self) -> bool {
        !self.negative_buckets.is_empty()
    }

    pub fn value_counts(&self) -> impl Iterator<Item = (f64, usize)> + '_ {
        self.negative_buckets
            .iter()
            .enumerate()
            .map(|(index, count)| {
                (
                    lower_boundary(self.actual_scale, self.bucket_start_offset as usize + index),
                    *count,
                )
            })
            .chain(
                self.positive_buckets
                    .iter()
                    .enumerate()
                    .map(|(index, count)| {
                        (
                            lower_boundary(
                                self.actual_scale,
                                self.bucket_start_offset as usize + index,
                            ),
                            *count,
                        )
                    }),
            )
    }

    fn get_mut_buckets_for_value(&mut self, value: f64) -> &mut VecDeque<usize> {
        let buckets = if value.is_sign_positive() {
            &mut self.positive_buckets
        } else {
            &mut self.negative_buckets
        };
        if buckets.is_empty() {
            // I could reserve these ahead of time, but it seems likely that many uses will have exclusively
            // positive or exclusively negative numbers - so this saves memory in those cases.
            buckets.reserve_exact(self.max_bucket_count as usize);
        }
        buckets
    }
}

/// treats negative numbers as positive - you gotta accumulate into a negative array
fn map_value_to_scale_index(scale: impl Into<i32>, raw_value: impl Into<f64>) -> usize {
    let value = raw_value.into().abs();
    let scale_factor = LOG2_E * 2_f64.powi(scale.into());
    (value.log(E) * scale_factor).floor() as usize
}

/// obviously only supports positive indices. If you want a negative boundary, flip the sign on the return value.
/// per the wonkadoo instructions found at: https://opentelemetry.io/docs/specs/otel/metrics/data-model/#exponentialhistogram
///   > The positive and negative ranges of the histogram are expressed separately. Negative values are mapped by
///   > their absolute value into the negative range using the same scale as the positive range. Note that in the
///   > negative range, therefore, histogram buckets use lower-inclusive boundaries.
fn lower_boundary(scale: impl Into<i32>, index: usize) -> f64 {
    let inverse_scale_factor = LN_2 * 2_f64.powi(-scale.into());
    (index as f64 * inverse_scale_factor).exp()
}

impl AbsorbDistribution for ExponentialHistogram {
    fn absorb(&mut self, distribution: crate::types::Distribution) {
        match distribution {
            Distribution::I64(i) => self.accumulate(i as f64),
            Distribution::I32(i) => self.accumulate(i),
            Distribution::U64(u) => self.accumulate(u as f64),
            Distribution::U32(u) => self.accumulate(u),
            Distribution::Collection(c) => {
                for i in c {
                    self.accumulate(i as f64)
                }
            }
            Distribution::Timer { nanos } => self.accumulate(nanos.load(Ordering::Relaxed) as f64),
        }
    }
}

#[cfg(test)]
mod test {
    use std::time::{Duration, Instant};

    use crate::pipeline::aggregation::exponential_histogram::{
        lower_boundary, map_value_to_scale_index,
    };

    use super::ExponentialHistogram;

    #[test]
    fn check_range() {
        assert_eq!(1275, map_value_to_scale_index(6, 1_000_000));
        assert_eq!(1275 + 160, map_value_to_scale_index(6, 5_650_000));

        assert_eq!(637, map_value_to_scale_index(5, 1_000_000));
        assert_eq!(637 + 160, map_value_to_scale_index(5, 32_000_000));
    }

    #[test]
    fn indices_scale_zero_positive_numbers() {
        let e = ExponentialHistogram::new(0);

        assert_eq!(0, map_value_to_scale_index(e.scale(), 0_f64));
        assert_value_lowerboundary(&e, 0, 1);
        assert_value_lowerboundary(&e, 1, 1);
        assert_value_lowerboundary(&e, 2, 2);
        assert_value_lowerboundary(&e, 3, 2);
        assert_value_lowerboundary(&e, 4, 4);
        assert_value_lowerboundary(&e, 7, 4);
        assert_value_lowerboundary(&e, 8, 4);
        assert_value_lowerboundary(&e, 8.1, 8);
    }

    #[test]
    fn indices_scale_zero_negative_numbers() {
        let e = ExponentialHistogram::new(0);

        assert_eq!(0, map_value_to_scale_index(e.scale(), 0_f64));
        assert_value_lowerboundary(&e, -0, 1);
        assert_value_lowerboundary(&e, -1, 1);
        assert_value_lowerboundary(&e, -2, 2);
        assert_value_lowerboundary(&e, -3, 2);
        assert_value_lowerboundary(&e, -4, 4);
        assert_value_lowerboundary(&e, -7, 4);
        assert_value_lowerboundary(&e, -8, 4);
        assert_value_lowerboundary(&e, -8.1, 8);
    }

    #[test]
    fn indices_scale_one_positive_numbers() {
        let e = ExponentialHistogram::new(1);

        assert_eq!(0, map_value_to_scale_index(e.scale(), 0_f64));
        assert_value_lowerboundary(&e, 0, 1);
        assert_value_lowerboundary(&e, 1, 1);
        assert_value_lowerboundary(&e, 2, 2);
        assert_value_lowerboundary(&e, 3, 2.828);
        assert_value_lowerboundary(&e, 4, 4);
        assert_value_lowerboundary(&e, 7, 5.657);
        assert_value_lowerboundary(&e, 8, 5.657);
        assert_value_lowerboundary(&e, 8.1, 8);
    }

    #[test]
    fn indices_scale_two_positive_numbers() {
        let e = ExponentialHistogram::new(2);

        assert_eq!(0, map_value_to_scale_index(e.scale(), 0_f64));
        assert_value_lowerboundary(&e, 0, 1);
        assert_value_lowerboundary(&e, 1, 1);
        assert_value_lowerboundary(&e, 2, 2);
        assert_value_lowerboundary(&e, 3, 2.828);
        assert_value_lowerboundary(&e, 4, 4);
        assert_value_lowerboundary(&e, 7, 6.727);
        assert_value_lowerboundary(&e, 8, 6.727);
        assert_value_lowerboundary(&e, 8.1, 8);
    }

    #[test]
    fn indices_scale_three_positive_numbers() {
        let e = ExponentialHistogram::new(3);

        assert_eq!(0, map_value_to_scale_index(e.scale(), 0_f64));
        assert_value_lowerboundary(&e, 0, 1);
        assert_value_lowerboundary(&e, 1, 1);
        assert_value_lowerboundary(&e, 2, 2);
        assert_value_lowerboundary(&e, 3, 2.828);
        assert_value_lowerboundary(&e, 4, 4);
        assert_value_lowerboundary(&e, 7, 6.727);
        assert_value_lowerboundary(&e, 8, 7.337);
        assert_value_lowerboundary(&e, 8.1, 8);
    }

    #[test]
    fn indices_scale_four_positive_numbers() {
        let e = ExponentialHistogram::new(4);

        assert_eq!(0, map_value_to_scale_index(e.scale(), 0_f64));
        assert_value_lowerboundary(&e, 0, 1);
        assert_value_lowerboundary(&e, 1, 1);
        assert_value_lowerboundary(&e, 2, 2);
        assert_value_lowerboundary(&e, 3, 2.954);
        assert_value_lowerboundary(&e, 4, 4);
        assert_value_lowerboundary(&e, 5, 4.967);
        assert_value_lowerboundary(&e, 6, 5.907);
        assert_value_lowerboundary(&e, 7, 6.727);
        assert_value_lowerboundary(&e, 8, 7.661);
        assert_value_lowerboundary(&e, 8.1, 8);
    }

    #[test]
    fn indices_scale_downgrade_positive_numbers() {
        //
        // -------- Start out with a fine-grained histogram --------
        //
        let mut e = ExponentialHistogram::new(8);

        e.accumulate(24_000_000);

        assert_eq!(
            8,
            e.scale(),
            "initial value should not change scale since it falls in the numeric range"
        );
        assert_eq!(
            81,
            e.positive_buckets.len(),
            "initial value should be in the middle"
        );
        assert_eq!(
            1, e.positive_buckets[80],
            "initial value should go in index 80 because that is halfway to 160"
        );
        assert_eq!(
            6196, e.bucket_start_offset,
            "bucket start offset should index into scale 8"
        );

        // assert some bucket boundaries for convenience
        assert_value_lowerboundary(&e, 24_000_000, 23984931.775);
        assert_value_lowerboundary(&e, 24_040_000, 23984931.775);
        assert_value_lowerboundary(&e, 24_050_000, 24049961.522);

        assert_eq_epsilon(
            19313750.368,
            lower_boundary(8, 6196),
            "lower boundary of histogram",
        );
        assert_eq_epsilon(
            29785874.896,
            lower_boundary(8, 6196 + 160),
            "upper boundary of histogram",
        );

        // Accumulate some data in a bucket's range
        for i in 0..40_000 {
            e.accumulate(24_000_000 + i);
        }
        assert_eq!(
            40001, e.positive_buckets[80],
            "initial value should go in index 80 because that is halfway to 160"
        );

        e.accumulate(24_050_000);
        assert_eq!(
            8,
            e.scale(),
            "a value in the next higher bucket should not change the scale"
        );
        assert_eq!(
            82,
            e.positive_buckets.len(),
            "bucket count should be able to increase densely when a new bucket value is observed"
        );
        assert_eq!(1, e.positive_buckets[81], "index 81 has a new count");
        assert_eq!(
            6196, e.bucket_start_offset,
            "bucket start offset does not change when adding a bucket in the same range"
        );

        // Poke at growth boundary conditions
        e.accumulate(23_984_000);
        assert_eq!(
            8,
            e.scale(),
            "a value in the next lower bucket should not change the scale"
        );
        assert_eq!(82, e.positive_buckets.len(), "bucket count should not increase when a new bucket value is observed within the covered range");
        assert_eq!(1, e.positive_buckets[79], "index 79 has a new count");
        assert_eq!(
            6196, e.bucket_start_offset,
            "bucket start offset does not change when using a bucket in the same range"
        );

        e.accumulate(19_313_750);
        assert_eq!(8, e.scale(), "a value below the covered range should not change the scale yet because there is room above the observed range to shift");
        assert_eq!(83, e.positive_buckets.len(), "bucket count should not increase when a new bucket value is observed within the covered range");
        assert_eq!(1, e.positive_buckets[0], "index 0 has a new count");
        assert_eq!(
            6195, e.bucket_start_offset,
            "bucket start offset changes because we rotated down 1 position"
        );
        assert_eq_epsilon(
            29705335.561,
            lower_boundary(8, 6195 + 160),
            "new upper boundary of histogram",
        );

        //
        // -------- Expand histogram range with a big number --------
        //
        e.accumulate(29_705_336);
        assert_eq!(
            3177,
            map_value_to_scale_index(7, 29_705_336_f64),
            "this value pushes the length of scale 7 also"
        );
        assert_eq!(7, e.scale(), "a value above the covered range should now change the scale because the lower end is populated while the upper end is beyond the range this scale can cover in 160 buckets");
        assert_eq!(
            160,
            e.positive_buckets.len(),
            "bucket count should be sensible after rescale"
        );
        assert_eq!(
            1,
            e.positive_buckets[e.positive_buckets.len() - 1],
            "last index has a new count"
        );
        assert_eq!(
            3018, e.bucket_start_offset,
            "bucket start offset changes because we scaled and rotated"
        );

        //
        // -------- Skip several zoom scale steps in a single accumulate --------
        //
        let recursive_scale_start_count = e.count();
        assert_eq!(
            2199023255551.996,
            lower_boundary(2, 164),
            "this value gets us down into scale 2"
        );
        assert_eq_epsilon(
            4.000,
            lower_boundary(2, 8),
            "this value gets us down into scale 2",
        );
        assert_eq_epsilon(
            4.757,
            lower_boundary(2, 9),
            "this value gets us down into scale 2",
        );
        // pin the bucket's low value, at scale 2's index 8. It's not in scale 2 yet though!
        e.accumulate(4.25);
        // now blow the range wide, way past scale 7, resulting in a recursive scale down from 7 to precision 2.
        e.accumulate(2_199_023_255_552_f64);
        assert_eq!(2, e.scale(), "this value range should force scale range 2");
        assert_eq!(8, e.bucket_start_offset, "bucket start offset should match the first element, since we rotated and grew out to the larger value");
        assert_eq!(
            1,
            e.positive_buckets[8 - 8],
            "this is the 4.0 bucket, and 4.25 should go in it."
        );
        assert_eq!(
            1,
            e.positive_buckets[164 - 8],
            "this is the bucket for the big numer."
        );
        assert_eq!(recursive_scale_start_count + 2, e.count(), "2 more reports were made. The histogram maintains every count across rescaling, even recursive rescaling");
    }

    /// Look for random index crashes
    #[test]
    fn fuzz() {
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(50) {
            let mut e = ExponentialHistogram::new(8);
            let start = Instant::now();
            while start.elapsed() < Duration::from_millis(1) {
                e.accumulate(1_000_000_000_000_000_f64 * rand::random::<f64>());
            }
        }
    }

    /// Look for random index crashes
    #[test]
    fn fuzz_negative() {
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(50) {
            let mut e = ExponentialHistogram::new(8);
            let start = Instant::now();
            while start.elapsed() < Duration::from_millis(1) {
                e.accumulate(-1_000_000_000_000_000_f64 * rand::random::<f64>());
            }
        }
    }

    #[track_caller]
    fn assert_value_lowerboundary(
        e: &ExponentialHistogram,
        value: impl Into<f64>,
        expected_lower_boundary: impl Into<f64>,
    ) {
        let observed_index = map_value_to_scale_index(e.scale(), value.into());
        let observed_boundary = lower_boundary(e.scale(), observed_index);
        assert_eq_epsilon(
            expected_lower_boundary.into(),
            observed_boundary,
            "boundary matches",
        );
    }

    #[track_caller]
    fn assert_eq_epsilon(j: f64, k: f64, message: &str) {
        const EPSILON: f64 = 1.0 / 128.0;
        let difference = (j - k).abs();
        assert!(
            difference < EPSILON,
            "{message}: {j} != {k} with epsilon {EPSILON}."
        );
    }
}
