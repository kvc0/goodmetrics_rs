use std::fmt::Display;

use super::Sink;

/// A metrics sink that just logs metrics and drops them
pub struct LoggingSink {
    log_level: log::Level,
}

impl Default for LoggingSink {
    fn default() -> Self {
        Self {
            log_level: log::Level::Info,
        }
    }
}

impl<T> Sink<T> for LoggingSink
where
    T: Display,
{
    fn accept(&self, metrics_ref: T)
    where
        T: Display,
    {
        log::log!(self.log_level, "Sunk: {}", metrics_ref)
    }
}
