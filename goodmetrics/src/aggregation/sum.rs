/// A basic aggregation.
#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Sum {
    /// A sum
    pub sum: i64,
}

impl std::fmt::Display for Sum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.sum)
    }
}

impl Sum {
    pub(crate) fn accumulate(&mut self, value: impl Into<i64>) {
        self.sum += value.into();
    }
}
