use std::{sync::Mutex, marker::PhantomData};

use super::Sink;

pub struct SerializingSink<TSink, TSunk> where TSink : Sink<TSunk> {
    downstream: Mutex<TSink>,
    _phantom: PhantomData<TSunk>,
}

impl <TSink, TSunk> SerializingSink<TSink, TSunk> where TSink : Sink<TSunk> {
    pub fn new(downstream: TSink) -> Self {
        Self {
            downstream: Mutex::new(downstream),
            _phantom: PhantomData::default(),
        }
    }
}

impl<TDownstream, TSunk> Sink<TSunk> for SerializingSink<TDownstream, TSunk> where TDownstream : Sink<TSunk> {
    fn accept(
        &self,
        to_sink: TSunk,
    ) {
        let downstream = self.downstream.lock().expect("failure in downstream metrics sink");
        downstream.accept(to_sink)
    }
}
