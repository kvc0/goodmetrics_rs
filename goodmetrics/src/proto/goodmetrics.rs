#[derive()]
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
#[derive()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MetricsReply {}
#[derive()]
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
#[derive()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Dimension {
    #[prost(oneof = "dimension::Value", tags = "1, 2, 3")]
    pub value: ::core::option::Option<dimension::Value>,
}
/// Nested message and enum types in `Dimension`.
pub mod dimension {
    #[derive()]
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
#[derive()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Measurement {
    #[prost(oneof = "measurement::Value", tags = "1, 2, 4, 5, 6, 7, 8")]
    pub value: ::core::option::Option<measurement::Value>,
}
/// Nested message and enum types in `Measurement`.
pub mod measurement {
    #[derive()]
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
#[derive()]
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
#[derive()]
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
#[derive()]
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
    #[derive()]
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
            D: TryInto<tonic::transport::Endpoint>,
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
        /// Limits the maximum size of a decoded message.
        ///
        /// Default: `4MB`
        #[must_use]
        pub fn max_decoding_message_size(mut self, limit: usize) -> Self {
            self.inner = self.inner.max_decoding_message_size(limit);
            self
        }
        /// Limits the maximum size of an encoded message.
        ///
        /// Default: `usize::MAX`
        #[must_use]
        pub fn max_encoding_message_size(mut self, limit: usize) -> Self {
            self.inner = self.inner.max_encoding_message_size(limit);
            self
        }
        pub async fn send_metrics(
            &mut self,
            request: impl tonic::IntoRequest<super::MetricsRequest>,
        ) -> std::result::Result<tonic::Response<super::MetricsReply>, tonic::Status> {
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
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("goodmetrics.Metrics", "SendMetrics"));
            self.inner.unary(req, path, codec).await
        }
    }
}
