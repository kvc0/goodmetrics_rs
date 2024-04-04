/// Base 10 significant-figures bucketing - toward 0
fn bucket_10<const FIGURES: u32>(value: i64) -> i64 {
    if value == 0 {
        return 0;
    }
    // TODO: use i64.log10 when it's promoted to stable https://github.com/rust-lang/rust/issues/70887
    let power = ((value.abs() as f64).log10().ceil() as i32 - FIGURES as i32).max(0);
    let magnitude = 10_f64.powi(power);

    value.signum()
        // -> truncate off magnitude by dividing it away
        // -> ceil() away from 0 in both directions due to abs
        * (value.abs() as f64 / magnitude).ceil() as i64
        // restore original magnitude raised to the next figure if necessary
        * magnitude as i64
}

/// Base 10 significant-figures bucketing - toward -inf
fn bucket_10_below<const FIGURES: u32>(value: i64) -> i64 {
    if value == 0 {
        return -1;
    }
    // TODO: use i64.log10 when it's promoted to stable https://github.com/rust-lang/rust/issues/70887
    let power = ((value.abs() as f64).log10().ceil() as i32 - FIGURES as i32).max(0);
    let magnitude = 10_f64.powi(power);

    (value.signum()
        // -> truncate off magnitude by dividing it away
        // -> ceil() away from 0 in both directions due to abs
        * (value.abs() as f64 / magnitude).ceil() as i64 - 1)
        // restore original magnitude raised to the next figure if necessary
        * magnitude as i64
}

/// Base 10 significant-figures bucketing - toward 0
pub fn bucket_10_2_sigfigs(value: i64) -> i64 {
    bucket_10::<2>(value)
}

/// Base 10 significant-figures bucketing - toward -inf
pub fn bucket_10_below_2_sigfigs(value: i64) -> i64 {
    bucket_10_below::<2>(value)
}

#[cfg(test)]
mod test {
    use crate::pipeline::aggregation::bucket::{bucket_10_2_sigfigs, bucket_10_below_2_sigfigs};

    #[test_log::test]
    fn test_bucket() {
        assert_eq!(0, bucket_10_2_sigfigs(0));
        assert_eq!(1, bucket_10_2_sigfigs(1));
        assert_eq!(-11, bucket_10_2_sigfigs(-11));

        assert_eq!(99, bucket_10_2_sigfigs(99));
        assert_eq!(100, bucket_10_2_sigfigs(100));
        assert_eq!(110, bucket_10_2_sigfigs(101));
        assert_eq!(110, bucket_10_2_sigfigs(109));
        assert_eq!(110, bucket_10_2_sigfigs(110));
        assert_eq!(120, bucket_10_2_sigfigs(111));

        assert_eq!(8000, bucket_10_2_sigfigs(8000));
        assert_eq!(8800, bucket_10_2_sigfigs(8799));
        assert_eq!(8800, bucket_10_2_sigfigs(8800));
        assert_eq!(8900, bucket_10_2_sigfigs(8801));

        assert_eq!(-8000, bucket_10_2_sigfigs(-8000));
        assert_eq!(-8800, bucket_10_2_sigfigs(-8799));
        assert_eq!(-8800, bucket_10_2_sigfigs(-8800));
        assert_eq!(-8900, bucket_10_2_sigfigs(-8801));
    }

    #[test_log::test]
    fn test_bucket_below() {
        assert_eq!(0, bucket_10_below_2_sigfigs(1));
        assert_eq!(-12, bucket_10_below_2_sigfigs(-11));

        assert_eq!(98, bucket_10_below_2_sigfigs(99));
        assert_eq!(99, bucket_10_below_2_sigfigs(100));
        assert_eq!(100, bucket_10_below_2_sigfigs(101));
        assert_eq!(100, bucket_10_below_2_sigfigs(109));
        assert_eq!(100, bucket_10_below_2_sigfigs(110));
        assert_eq!(110, bucket_10_below_2_sigfigs(111));

        assert_eq!(7900, bucket_10_below_2_sigfigs(8000));
        assert_eq!(8700, bucket_10_below_2_sigfigs(8799));
        assert_eq!(8700, bucket_10_below_2_sigfigs(8800));
        assert_eq!(8800, bucket_10_below_2_sigfigs(8801));

        assert_eq!(-8100, bucket_10_below_2_sigfigs(-8000));
        assert_eq!(-8900, bucket_10_below_2_sigfigs(-8799));
        assert_eq!(-8900, bucket_10_below_2_sigfigs(-8800));
        assert_eq!(-9000, bucket_10_below_2_sigfigs(-8801));
    }
}
