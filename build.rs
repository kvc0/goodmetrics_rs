use std::env;
use std::path::PathBuf;

fn main() {
    let _out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    tonic_build::configure()
        .build_server(false)
        .type_attribute(".", "#[derive(serde::Deserialize, serde::Serialize, Eq)]")
        .compile(&["proto/metrics/goodmetrics.proto"], &["proto"])
        .unwrap();

    tonic_build::configure()
        .build_server(false)
        .type_attribute("InstrumentationScope", "#[derive(Eq)]")
        .type_attribute("InstrumentationLibrary", "#[derive(Eq)]")
        .type_attribute("Buckets", "#[derive(Eq)]")
        .type_attribute("ExportMetricsServiceResponse", "#[derive(Eq)]")
        .compile(
            &[
                "proto/opentelemetry/proto/metrics/v1/metrics.proto",
                "proto/opentelemetry/proto/collector/metrics/v1/metrics_service.proto",
                "proto/opentelemetry/proto/common/v1/common.proto",
            ],
            &["proto"],
        )
        .unwrap();
}
