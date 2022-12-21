use std::time::{SystemTime, UNIX_EPOCH};

pub mod channel_connection;
pub mod goodmetrics_downstream;
pub mod opentelemetry_downstream;

pub trait EpochTime {
    fn nanos_since_epoch(&self) -> u64;
}

impl EpochTime for SystemTime {
    fn nanos_since_epoch(&self) -> u64 {
        self.duration_since(UNIX_EPOCH)
            .expect("could not get system time")
            .as_nanos() as u64
    }
}
