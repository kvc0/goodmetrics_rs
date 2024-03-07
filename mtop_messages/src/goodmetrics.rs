#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MetricsRequest {
    #[prost(map = "string, message", tag = "1")]
    pub shared_dimensions: ::std::collections::HashMap<
        ::prost::alloc::string::String,
        Dimension,
    >,
    #[prost(message, repeated, tag = "2")]
    pub metrics: ::prost::alloc::vec::Vec<Datum>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MetricsReply {}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Datum {
    /// You should use lowercase_snake_case for this.
    /// If you don't, you may be surprised to see lowercase_snake_case-ification in your database.
    ///
    /// Use names like: service_operation
    #[prost(string, tag = "1")]
    pub metric: ::prost::alloc::string::String,
    #[prost(uint64, tag = "2")]
    pub unix_nanos: u64,
    #[prost(map = "string, message", tag = "3")]
    pub dimensions: ::std::collections::HashMap<
        ::prost::alloc::string::String,
        Dimension,
    >,
    #[prost(map = "string, message", tag = "4")]
    pub measurements: ::std::collections::HashMap<
        ::prost::alloc::string::String,
        Measurement,
    >,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Dimension {
    #[prost(oneof = "dimension::Value", tags = "1, 2, 3")]
    pub value: ::core::option::Option<dimension::Value>,
}
/// Nested message and enum types in `Dimension`.
pub mod dimension {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Value {
        #[prost(string, tag = "1")]
        String(::prost::alloc::string::String),
        #[prost(uint64, tag = "2")]
        Number(u64),
        #[prost(bool, tag = "3")]
        Boolean(bool),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Measurement {
    #[prost(oneof = "measurement::Value", tags = "1, 2, 4, 5, 6, 7, 8")]
    pub value: ::core::option::Option<measurement::Value>,
}
/// Nested message and enum types in `Measurement`.
pub mod measurement {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Value {
        #[prost(int64, tag = "1")]
        I64(i64),
        #[prost(int32, tag = "2")]
        I32(i32),
        #[prost(double, tag = "4")]
        F64(f64),
        #[prost(float, tag = "5")]
        F32(f32),
        #[prost(message, tag = "6")]
        StatisticSet(super::StatisticSet),
        #[prost(message, tag = "7")]
        Histogram(super::Histogram),
        #[prost(message, tag = "8")]
        Tdigest(super::TDigest),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StatisticSet {
    #[prost(double, tag = "1")]
    pub minimum: f64,
    #[prost(double, tag = "2")]
    pub maximum: f64,
    #[prost(double, tag = "3")]
    pub samplesum: f64,
    #[prost(uint64, tag = "4")]
    pub samplecount: u64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Histogram {
    #[prost(map = "int64, uint64", tag = "1")]
    pub buckets: ::std::collections::HashMap<i64, u64>,
}
/// For use with T-Digests. You should be able to construct one of these
/// from libraries in various languages, and they should be relatively
/// easily convertible to downstream representations (like timescale's
/// string representation of tdigests, which is an adaptation of the Rust
/// tdigest crate's representation)
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TDigest {
    #[prost(message, repeated, tag = "1")]
    pub centroids: ::prost::alloc::vec::Vec<t_digest::Centroid>,
    #[prost(double, tag = "2")]
    pub sum: f64,
    #[prost(uint64, tag = "3")]
    pub count: u64,
    #[prost(double, tag = "4")]
    pub max: f64,
    #[prost(double, tag = "5")]
    pub min: f64,
}
/// Nested message and enum types in `TDigest`.
pub mod t_digest {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Centroid {
        #[prost(double, tag = "1")]
        pub mean: f64,
        #[prost(uint64, tag = "2")]
        pub weight: u64,
    }
}
/// Generated client implementations.
pub mod metrics_client {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    use tonic::codegen::http::Uri;
    #[derive(Debug, Clone)]
    pub struct MetricsClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl MetricsClient<tonic::transport::Channel> {
        /// Attempt to create a new client by connecting to a given endpoint.
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: std::convert::TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> MetricsClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::Error: Into<StdError>,
        T::ResponseBody: Body<Data = Bytes> + Send + 'static,
        <T::ResponseBody as Body>::Error: Into<StdError> + Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }
        pub fn with_origin(inner: T, origin: Uri) -> Self {
            let inner = tonic::client::Grpc::with_origin(inner, origin);
            Self { inner }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> MetricsClient<InterceptedService<T, F>>
        where
            F: tonic::service::Interceptor,
            T::ResponseBody: Default,
            T: tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
                Response = http::Response<
                    <T as tonic::client::GrpcService<tonic::body::BoxBody>>::ResponseBody,
                >,
            >,
            <T as tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
            >>::Error: Into<StdError> + Send + Sync,
        {
            MetricsClient::new(InterceptedService::new(inner, interceptor))
        }
        /// Compress requests with the given encoding.
        ///
        /// This requires the server to support it otherwise it might respond with an
        /// error.
        #[must_use]
        pub fn send_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.inner = self.inner.send_compressed(encoding);
            self
        }
        /// Enable decompressing responses.
        #[must_use]
        pub fn accept_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.inner = self.inner.accept_compressed(encoding);
            self
        }
        pub async fn send_metrics(
            &mut self,
            request: impl tonic::IntoRequest<super::MetricsRequest>,
        ) -> Result<tonic::Response<super::MetricsReply>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/goodmetrics.Metrics/SendMetrics",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
    }
}
/// Generated server implementations.
pub mod metrics_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    /// Generated trait containing gRPC methods that should be implemented for use with MetricsServer.
    #[async_trait]
    pub trait Metrics: Send + Sync + 'static {
        async fn send_metrics(
            &self,
            request: tonic::Request<super::MetricsRequest>,
        ) -> Result<tonic::Response<super::MetricsReply>, tonic::Status>;
    }
    #[derive(Debug)]
    pub struct MetricsServer<T: Metrics> {
        inner: _Inner<T>,
        accept_compression_encodings: EnabledCompressionEncodings,
        send_compression_encodings: EnabledCompressionEncodings,
    }
    struct _Inner<T>(Arc<T>);
    impl<T: Metrics> MetricsServer<T> {
        pub fn new(inner: T) -> Self {
            Self::from_arc(Arc::new(inner))
        }
        pub fn from_arc(inner: Arc<T>) -> Self {
            let inner = _Inner(inner);
            Self {
                inner,
                accept_compression_encodings: Default::default(),
                send_compression_encodings: Default::default(),
            }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> InterceptedService<Self, F>
        where
            F: tonic::service::Interceptor,
        {
            InterceptedService::new(Self::new(inner), interceptor)
        }
        /// Enable decompressing requests with the given encoding.
        #[must_use]
        pub fn accept_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.accept_compression_encodings.enable(encoding);
            self
        }
        /// Compress responses with the given encoding, if the client supports it.
        #[must_use]
        pub fn send_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.send_compression_encodings.enable(encoding);
            self
        }
    }
    impl<T, B> tonic::codegen::Service<http::Request<B>> for MetricsServer<T>
    where
        T: Metrics,
        B: Body + Send + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = std::convert::Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            let inner = self.inner.clone();
            match req.uri().path() {
                "/goodmetrics.Metrics/SendMetrics" => {
                    #[allow(non_camel_case_types)]
                    struct SendMetricsSvc<T: Metrics>(pub Arc<T>);
                    impl<T: Metrics> tonic::server::UnaryService<super::MetricsRequest>
                    for SendMetricsSvc<T> {
                        type Response = super::MetricsReply;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::MetricsRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).send_metrics(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SendMetricsSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => {
                    Box::pin(async move {
                        Ok(
                            http::Response::builder()
                                .status(200)
                                .header("grpc-status", "12")
                                .header("content-type", "application/grpc")
                                .body(empty_body())
                                .unwrap(),
                        )
                    })
                }
            }
        }
    }
    impl<T: Metrics> Clone for MetricsServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self {
                inner,
                accept_compression_encodings: self.accept_compression_encodings,
                send_compression_encodings: self.send_compression_encodings,
            }
        }
    }
    impl<T: Metrics> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: Metrics> tonic::server::NamedService for MetricsServer<T> {
        const NAME: &'static str = "goodmetrics.Metrics";
    }
}
