//! Types related to emitting metrics to collectors

use std::time::{SystemTime, UNIX_EPOCH};

mod channel_connection;
mod goodmetrics_downstream;
mod opentelemetry_downstream;

pub use channel_connection::{get_client, ChannelType};
pub use goodmetrics_downstream::{GoodmetricsBatcher, GoodmetricsDownstream};
pub use opentelemetry_downstream::{OpenTelemetryDownstream, OpentelemetryBatcher};

pub(crate) type StdError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// A provider of unix epoch nanos
pub trait EpochTime {
    /// return nanos since the unix epoch
    fn nanos_since_epoch(&self) -> u64;
}

impl EpochTime for SystemTime {
    fn nanos_since_epoch(&self) -> u64 {
        self.duration_since(UNIX_EPOCH)
            .expect("could not get system time")
            .as_nanos() as u64
    }
}
