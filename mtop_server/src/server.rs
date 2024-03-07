use std::{pin::Pin, sync::Arc, task::{Context, Poll}};

use futures::{future::BoxFuture, FutureExt};
use goodmetrics::pipeline::aggregator::AggregatedMetricsMap;
use mtop_messages::mtop::{mtop_server, SubscribeRequest, MtopMessage};
use tokio::sync::broadcast::{Sender, Receiver};
use tokio_stream::wrappers::{ReceiverStream, BroadcastStream};

struct Server {
    metrics_stream: Sender<Arc<AggregatedMetricsMap>>
}

impl mtop_server::Mtop for Server {
    type SubscribeStream = MtopStream;

    fn subscribe<'life0, 'async_trait>(
        &'life0 self,
        request: tonic::Request<SubscribeRequest>
    ) -> BoxFuture<Result<tonic::Response<Self::SubscribeStream>, tonic::Status>>
    where
        'life0: 'async_trait,
        Self: 'async_trait
    {
        let stream = MtopStream { receiver: self.metrics_stream.subscribe().into() };
        async move {
            Ok(tonic::Response::new(stream))
        }.boxed()
    }
}

struct MtopStream {
    receiver: BroadcastStream<Arc<AggregatedMetricsMap>>,
}

impl futures::Stream for MtopStream {
    type Item = Result<MtopMessage, tonic::Status>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.recv()
        todo!()
    }
}
