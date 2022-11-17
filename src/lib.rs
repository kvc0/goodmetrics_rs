pub mod allocator;
pub mod downstream;
pub mod metrics;
pub mod metrics_factory;
pub mod pipeline;
pub mod types;

// opentelemetry uses poor enum variant names
#[allow(clippy::enum_variant_names)]
pub(crate) mod proto {
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
