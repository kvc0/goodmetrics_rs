use std::path::PathBuf;
#[allow(clippy::unwrap_used)]
fn main() {
    let out_dir = PathBuf::from("../lib/src/proto");
    let proto_dir = "../proto";

    eprintln!("Hi brave developer! If you are changing protos and goodmetrics fails to build, please retry 1 time.");
    eprintln!("Cargo currently does not have a nice way for me to express a dependency order between these 2");
    eprintln!("workspace projects - because this project is _specifically_ supposed to not be a Cargo dependency.");
    eprintln!("I did this so users don't need to have protoc when compiling goodmetrics!");

    tonic_build::configure()
        .build_server(false)
        .type_attribute(".", "#[derive()]")
        .out_dir(out_dir.clone())
        .compile(
            &[format!("{proto_dir}/metrics/goodmetrics.proto")],
            &[proto_dir],
        )
        .unwrap();

    tonic_build::configure()
        .build_server(false)
        .type_attribute("InstrumentationScope", "#[derive(Eq)]")
        .type_attribute("InstrumentationLibrary", "#[derive(Eq)]")
        .type_attribute("Buckets", "#[derive(Eq)]")
        .type_attribute("ExportMetricsServiceResponse", "#[derive(Eq)]")
        .out_dir(out_dir)
        .compile(
            &[
                format!("{proto_dir}/opentelemetry/proto/metrics/v1/metrics.proto"),
                format!(
                    "{proto_dir}/opentelemetry/proto/collector/metrics/v1/metrics_service.proto"
                ),
                format!("{proto_dir}/opentelemetry/proto/common/v1/common.proto"),
            ],
            &[proto_dir],
        )
        .unwrap();

    println!("cargo:rerun-if-changed=../proto");
}
