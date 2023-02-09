// opentelemetry uses poor enum variant names
#[allow(clippy::enum_variant_names)]

pub mod goodmetrics;

pub mod opentelemetry {
    pub mod collector {
        pub mod metrics {
            pub mod v1 {
                include!("opentelemetry.proto.collector.metrics.v1.rs");
            }
        }
    }
    pub mod metrics {
        pub mod v1 {
            include!("opentelemetry.proto.metrics.v1.rs");
        }
    }
    pub mod common {
        pub mod v1 {
            include!("opentelemetry.proto.common.v1.rs");
        }
    }
    pub mod resource {
        pub mod v1 {
            include!("opentelemetry.proto.resource.v1.rs");
        }
    }
}
