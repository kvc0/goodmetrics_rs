use std::{collections::hash_map, sync::mpsc::Receiver, time::Duration};

use crate::{
    pipeline::aggregating_sink::DimensionedMeasurementsMap,
    proto::opentelemetry::{
        collector::metrics::v1::{
            metrics_service_client::MetricsServiceClient, ExportMetricsServiceRequest,
        },
        common::v1::InstrumentationScope,
        metrics::v1::{Metric, ResourceMetrics, ScopeMetrics},
    },
    types::Name,
};

use super::channel_connection::ChannelType;

pub struct OpenTelemetryDownstream {
    client: MetricsServiceClient<ChannelType>,
}

const VERSION: Option<&str> = option_env!("CARGO_PKG_VERSION");

impl OpenTelemetryDownstream {
    pub fn new(channel: ChannelType) -> Self {
        let client: MetricsServiceClient<ChannelType> = MetricsServiceClient::new(channel);

        OpenTelemetryDownstream { client }
    }

    pub fn send_batches_forever(&mut self, receiver: Receiver<Vec<Metric>>) {
        let interval = Duration::from_secs(1);
        loop {
            match receiver.recv_timeout(interval) {
                Ok(batch) => {
                    let future = self.client.export(ExportMetricsServiceRequest {
                        resource_metrics: vec![ResourceMetrics {
                            resource: None,
                            schema_url: "".to_string(),
                            scope_metrics: vec![ScopeMetrics {
                                scope: Some(InstrumentationScope {
                                    name: "goodmetrics".to_string(),
                                    version: VERSION.unwrap_or("unknown").to_string(),
                                }),
                                schema_url: "".to_string(),
                                metrics: batch,
                            }],
                        }],
                    });
                    let result = futures::executor::block_on(future);
                    log::debug!("sent metrics: {result:?}");
                }
                Err(timeout) => {
                    log::info!("no metrics activity: {timeout}");
                }
            }
        }
    }
}

pub fn create_preaggregated_opentelemetry_batch(
    _batch: hash_map::Drain<'_, Name, DimensionedMeasurementsMap>,
) -> Vec<Metric> {
    todo!()
}
