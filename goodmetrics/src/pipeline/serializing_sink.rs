use std::sync::Mutex;

use super::Sink;

/// A metrics sink that provides external synchronization for a downstream sink.
pub struct SerializingSink<TSink> {
    downstream: Mutex<TSink>,
}

impl<TSink> SerializingSink<TSink> {
    /// Make a new serializing sink. The downstream is wrapped in a mutex.
    pub fn new(downstream: TSink) -> Self {
        Self {
            downstream: Mutex::new(downstream),
        }
    }
}

impl<TDownstream, TSunk> Sink<TSunk> for SerializingSink<TDownstream>
where
    TDownstream: Sink<TSunk>,
{
    fn accept(&self, to_sink: TSunk) {
        let downstream = self
            .downstream
            .lock()
            .expect("failure in downstream metrics sink");
        downstream.accept(to_sink)
    }
}
