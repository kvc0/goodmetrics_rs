use tokio::sync::mpsc;

use super::Sink;

#[derive(Debug)]
pub struct StreamSink<TMetricsRef> {
    queue: mpsc::Sender<TMetricsRef>,
}

impl<TMetricsRef> Clone for StreamSink<TMetricsRef> {
    fn clone(&self) -> Self {
        Self {
            queue: self.queue.clone(),
        }
    }
}

impl<TMetricsRef> StreamSink<TMetricsRef> {
    pub fn new() -> (Self, mpsc::Receiver<TMetricsRef>) {
        let (sender, receiver) = mpsc::channel(1024);

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
