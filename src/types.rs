#[derive(Debug)]
pub enum Dimension {
    Str(&'static str),
    String(String),
    Number(u64),
    Boolean(bool),
}

#[derive(Debug, Eq, Hash, PartialEq)]
pub enum Name {
    Str(&'static str),
    String(String),
}

#[derive(Debug)]
pub enum Measurement {
    I64(i64),
    I32(i32),
    F64(f64),
    F32(f32),
    // Encapsulates observations of a raw value.
    // Bucketing and aggregation happens in the pipeline.
    // From<&[u8]> is not defined because it costs a copy.
    // Also, this unconditionally uses the global allocator.
    // PR to plumb the allocator type out would be welcome.
    Distribution(Vec<u64>),
    // 1 observation of a distribution.
    DistributionObservation(u64),
}

impl From<&'static str> for Name {
    #[inline]
    fn from(s: &'static str) -> Self {
        Name::Str(s)
    }
}

impl From<String> for Name {
    #[inline]
    fn from(s: String) -> Self {
        Name::String(s)
    }
}

impl From<&'static str> for Dimension {
    #[inline]
    fn from(s: &'static str) -> Self {
        Dimension::Str(s)
    }
}

impl From<String> for Dimension {
    #[inline]
    fn from(s: String) -> Self {
        Dimension::String(s)
    }
}

impl From<u64> for Dimension {
    #[inline]
    fn from(n: u64) -> Self {
        Dimension::Number(n)
    }
}

impl From<u32> for Dimension {
    #[inline]
    fn from(n: u32) -> Self {
        Dimension::Number(n as u64)
    }
}

impl From<u8> for Dimension {
    #[inline]
    fn from(n: u8) -> Self {
        Dimension::Number(n as u64)
    }
}

impl From<bool> for Dimension {
    #[inline]
    fn from(b: bool) -> Self {
        Dimension::Boolean(b)
    }
}

impl From<i64> for Measurement {
    #[inline]
    fn from(n: i64) -> Self {
        Measurement::I64(n)
    }
}

impl From<i32> for Measurement {
    #[inline]
    fn from(n: i32) -> Self {
        Measurement::I32(n)
    }
}

impl From<f64> for Measurement {
    #[inline]
    fn from(n: f64) -> Self {
        Measurement::F64(n)
    }
}

impl From<f32> for Measurement {
    #[inline]
    fn from(n: f32) -> Self {
        Measurement::F32(n)
    }
}

impl From<Vec<u64>> for Measurement {
    #[inline]
    fn from(n: Vec<u64>) -> Self {
        Measurement::Distribution(n)
    }
}

impl From<u64> for Measurement {
    #[inline]
    fn from(n: u64) -> Self {
        Measurement::DistributionObservation(n)
    }
}
