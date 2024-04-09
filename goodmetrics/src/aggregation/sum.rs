/// A basic aggregation.
#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Sum {
    /// A sum
    pub sum: i64,
}

impl Sum {
    pub(crate) fn accumulate(&mut self, value: impl Into<i64>) {
        self.sum += value.into();
    }
}
