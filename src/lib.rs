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

pub mod allocator;
pub mod downstream;
pub mod metrics;
pub mod metrics_factory;
pub mod pipeline;
pub mod types;

// opentelemetry uses poor enum variant names
#[allow(clippy::enum_variant_names)]
pub(crate) mod proto {
    pub mod goodmetrics {
        tonic::include_proto!("goodmetrics");
    }
    pub mod opentelemetry {
        pub mod collector {
            pub mod metrics {
                pub mod v1 {
                    tonic::include_proto!("opentelemetry.proto.collector.metrics.v1");
                }
            }
        }
        pub mod metrics {
            pub mod v1 {
                tonic::include_proto!("opentelemetry.proto.metrics.v1");
            }
        }
        pub mod common {
            pub mod v1 {
                tonic::include_proto!("opentelemetry.proto.common.v1");
            }
        }
        pub mod resource {
            pub mod v1 {
                tonic::include_proto!("opentelemetry.proto.resource.v1");
            }
        }
    }
}
