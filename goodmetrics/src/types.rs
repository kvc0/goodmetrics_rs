use std::{
    fmt::Display,
    sync::{atomic::AtomicUsize, Arc},
    time::Duration,
};

/// The value part of a dimension's key/value pair.
#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub enum Dimension {
    /// A static string dimension. Feel really good about these.
    Str(&'static str),
    /// Avoid String dimensions when you can, as clones can add up.
    String(String),
    /// If you have a rarely-changing identifier you could consider using shared memory
    /// instead of cloning repeatedly.
    Shared(Arc<String>),
    /// A number dimension.
    Number(u64),
    /// A boolean dimension.
    Boolean(bool),
}

impl Display for Dimension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Dimension::Str(s) => write!(f, "{s}"),
            Dimension::String(s) => write!(f, "{s}"),
            Dimension::Shared(s) => write!(f, "{s}"),
            Dimension::Number(n) => write!(f, "{n}"),
            Dimension::Boolean(b) => write!(f, "{b}"),
        }
    }
}

/// An identifier for various things.
#[derive(Debug, Eq, Hash, PartialEq, PartialOrd, Ord, Clone)]
pub enum Name {
    /// A static string Name.
    Str(&'static str),
    /// A String name. Avoid these when you can, because clones can add up.
    String(String),
    /// If you have a rarely-changing identifier you could consider using shared memory
    /// instead of cloning repeatedly.
    ///
    /// Aside from memory, a rough latency rule of thumb is:
    /// * String: 100%
    /// * Shared: 50%
    /// * Static: 30%
    Shared(Arc<String>),
}

impl Name {
    /// an &str view of the name
    pub fn as_str(&self) -> &str {
        match self {
            Name::Str(s) => s,
            Name::String(s) => s,
            Name::Shared(s) => s,
        }
    }
}

impl From<Name> for String {
    fn from(name: Name) -> Self {
        match name {
            Name::Str(s) => s.to_owned(),
            Name::String(s) => s,
            Name::Shared(s) => {
                std::sync::Arc::<String>::try_unwrap(s).unwrap_or_else(|this| this.to_string())
            }
        }
    }
}

impl Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Name::Str(s) => f.write_str(s),
            Name::String(s) => f.write_str(s),
            Name::Shared(s) => f.write_str(s),
        }
    }
}

/// Abstraction of measurement kinds - the more unary observation-oriented kind
/// and the distribution kind.
#[derive(Debug)]
pub enum Measurement {
    /// A single value
    Observation(Observation),
    /// A value to be grouped into a distribution
    Distribution(Distribution),
}

/// Individual values
#[derive(Debug)]
pub enum Observation {
    /// an integer value
    I64(i64),
    /// an integer value
    I32(i32),
    /// an unsigned integer value
    U64(u64),
    /// an unsigned integer value
    U32(u32),
    /// a floating point value
    F64(f64),
    /// a floating point value
    F32(f32),
}

/// Values able to be collected into a distribution
#[derive(Debug)]
pub enum Distribution {
    /// an integer distribution value
    I64(i64),
    /// an integer distribution value
    I32(i32),
    /// an unsigned integer distribution value
    U64(u64),
    /// an unsigned integer distribution value
    U32(u32),
    /// Encapsulates observations of a raw value.
    /// Bucketing and aggregation happens in the pipeline.
    /// From<&[u8]> is not defined because it costs a copy.
    /// Also, this unconditionally uses the global allocator.
    /// PR to plumb the allocator type out would be welcome.
    Collection(Vec<i64>),
    /// A helper for recording a distribution of time. This is
    /// shared by a Timer with a Drop implementation and the
    /// Metrics object for it. I don't enforce that the timer
    /// is dropped before the metrics, because tokio::spawn
    /// requires that the closure is owned for 'static. So
    /// extremely rigorous correctness takes a backseat to
    /// usability here.
    Timer {
        /// The number of nanoseconds this value represents.
        nanos: Arc<AtomicUsize>,
    },
}

impl From<&Observation> for f64 {
    fn from(value: &Observation) -> Self {
        match value {
            Observation::I64(n) => *n as f64,
            Observation::I32(n) => *n as f64,
            Observation::U64(n) => *n as f64,
            Observation::U32(n) => *n as f64,
            Observation::F64(n) => *n,
            Observation::F32(n) => *n as f64,
        }
    }
}

impl From<&'static str> for Name {
    #[inline]
    fn from(s: &'static str) -> Self {
        Self::Str(s)
    }
}

impl From<String> for Name {
    #[inline]
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<Arc<String>> for Name {
    #[inline]
    fn from(s: Arc<String>) -> Self {
        Self::Shared(s)
    }
}

impl From<&'static str> for Dimension {
    #[inline]
    fn from(s: &'static str) -> Self {
        Self::Str(s)
    }
}

impl From<String> for Dimension {
    #[inline]
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<Arc<String>> for Dimension {
    #[inline]
    fn from(s: Arc<String>) -> Self {
        Self::Shared(s)
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

impl From<i64> for Observation {
    #[inline]
    fn from(n: i64) -> Self {
        Observation::I64(n)
    }
}

impl From<i32> for Observation {
    #[inline]
    fn from(n: i32) -> Self {
        Observation::I32(n)
    }
}

impl From<u64> for Observation {
    #[inline]
    fn from(n: u64) -> Self {
        Observation::U64(n)
    }
}

impl From<u32> for Observation {
    #[inline]
    fn from(n: u32) -> Self {
        Observation::U32(n)
    }
}

impl From<f64> for Observation {
    #[inline]
    fn from(n: f64) -> Self {
        Observation::F64(n)
    }
}

impl From<f32> for Observation {
    #[inline]
    fn from(n: f32) -> Self {
        Observation::F32(n)
    }
}

impl From<i64> for Distribution {
    #[inline]
    fn from(n: i64) -> Self {
        Distribution::I64(n)
    }
}

impl From<Observation> for i64 {
    fn from(o: Observation) -> Self {
        match o {
            Observation::I64(i) => i,
            Observation::I32(i) => i.into(),
            Observation::U64(u) => u as i64,
            Observation::U32(u) => u.into(),
            Observation::F64(f) => f as i64,
            Observation::F32(f) => f as i64,
        }
    }
}

impl From<i32> for Distribution {
    #[inline]
    fn from(n: i32) -> Self {
        Distribution::I32(n)
    }
}

impl From<u64> for Distribution {
    #[inline]
    fn from(n: u64) -> Self {
        Distribution::U64(n)
    }
}

impl From<u32> for Distribution {
    #[inline]
    fn from(n: u32) -> Self {
        Distribution::U32(n)
    }
}

impl From<Vec<i64>> for Distribution {
    #[inline]
    fn from(n: Vec<i64>) -> Self {
        Distribution::Collection(n)
    }
}

impl From<Duration> for Distribution {
    #[inline]
    fn from(duration: Duration) -> Self {
        Distribution::U64(duration.as_nanos() as u64)
    }
}
