fn main() {
    tonic_build::configure()
        .build_server(false)
        .type_attribute(".", "#[derive()]")
        .compile(&["proto/metrics/goodmetrics.proto"], &["proto"])
        .expect("should be able to compile goodmetrics protos");

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
        .expect("should be able to compile opentelemetry protos");
}
