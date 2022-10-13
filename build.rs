use std::env;
use std::path::PathBuf;

fn main() {
    let _out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    tonic_build::configure()
        .build_server(false)
        .type_attribute(".", "#[derive(serde::Deserialize, serde::Serialize)]")
        .compile(&["proto/metrics/goodmetrics.proto"], &["proto"])
        .unwrap();

    tonic_build::configure()
        .build_server(false)
        // .type_attribute(".", "#[derive(Debug)]")
        .compile(
            &[
                "proto/opentelemetry/proto/metrics/v1/metrics.proto",
                "proto/opentelemetry/proto/collector/metrics/v1/metrics_service.proto",
            ],
            &["proto"],
        )
        .unwrap();
}
