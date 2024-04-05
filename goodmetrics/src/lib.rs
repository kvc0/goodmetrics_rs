//! A Rust implementation of goodmetrics; a low-overhead, expressive metrics
//! infrastructure built for web services.
//!
//! [`goodmetrics`] is a metrics recording toolbox built on [tonic] and [gRPC].
//! It focuses on your service first - metrics are never more important than your
//! users. After that, **high performance**, **low predictable overhead**, and
//! **ease of maintenance** are prioritized.
//!
//! # Examples
//!
//! Some basic examples can be found under `src/benches`.
//!
//! # Getting Started
//!
//! Check out [goodmetrics](https://github.com/kvc0/goodmetrics) documentation for
//! database setup and general ecosystem information.
//!
//! # Feature Flags
//!

#[deny(missing_docs)]
pub mod aggregation;
#[deny(missing_docs)]
pub mod allocator;
#[deny(missing_docs)]
pub mod downstream;
#[deny(missing_docs)]
mod gauge;
#[deny(missing_docs)]
mod gauge_group;
#[deny(missing_docs)]
mod metrics;
pub mod metrics_factory;
pub mod pipeline;
pub mod types;

pub use gauge::{GaugeDimensions, StatisticSetGauge};
pub use gauge_group::GaugeGroup;
pub use metrics::{DimensionGuard, Metrics, MetricsBehavior, Timer};

/// Internal generated types - ideally you shouldn't need to do much with them.
/// Nevertheless, they are exported in case you need them.
pub mod proto;
