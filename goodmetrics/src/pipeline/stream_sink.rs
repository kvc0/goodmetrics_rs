use std::sync::mpsc;

use super::Sink;

/// A metrics sink that queues metrics in an mpsc for a downstream
#[derive(Debug)]
pub struct StreamSink<TMetricsRef> {
    queue: mpsc::SyncSender<TMetricsRef>,
}

impl<TMetricsRef> Clone for StreamSink<TMetricsRef> {
    fn clone(&self) -> Self {
        Self {
            queue: self.queue.clone(),
        }
    }
}

impl<TMetricsRef> StreamSink<TMetricsRef> {
    /// Create a new stream sink.
    /// StreamSink is a Sink suitable for wiring to multiple metrics factories.
    /// The receiver is suitable for consuming into an aggregator or batching into raw metrics.
    pub fn new() -> (Self, mpsc::Receiver<TMetricsRef>) {
        let (sender, receiver) = mpsc::sync_channel(1024);

        (Self { queue: sender }, receiver)
    }
}

impl<TMetricsRef> Sink<TMetricsRef> for StreamSink<TMetricsRef> {
    fn accept(&self, to_sink: TMetricsRef) {
        match self.queue.try_send(to_sink) {
            Ok(_) => (),
            Err(e) => {
                log::debug!("could not send metrics: {e:?}");
            }
        }
    }
}
