use std::{str::FromStr, sync::Arc};

use hyper::{client::HttpConnector, http, Body, Error, Request, Response, Uri};
use tokio_rustls::rustls::{client::ServerCertVerifier, ClientConfig, RootCertStore};
use tonic::body::BoxBody;
use tower::{buffer::Buffer, util::BoxService, ServiceExt};

pub type ChannelType =
    Buffer<BoxService<Request<BoxBody>, Response<Body>, Error>, Request<BoxBody>>;

pub async fn get_channel(
    endpoint: &str,
    insecure: bool,
) -> Result<ChannelType, Box<dyn std::error::Error>> {
    let mut tls = ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(RootCertStore::empty())
        .with_no_client_auth();
    if insecure {
        tls.dangerous()
            .set_certificate_verifier(Arc::new(StupidVerifier {}));
    }

    let mut http_connector = HttpConnector::new();
    http_connector.enforce_http(false);
    let https_connector = tower::ServiceBuilder::new()
        .layer_fn(move |http_connector| {
            let tls = tls.clone();

            hyper_rustls::HttpsConnectorBuilder::new()
                .with_tls_config(tls)
                .https_or_http()
                .enable_http2()
                .wrap_connector(http_connector)
        })
        .service(http_connector);

    let https_client = hyper::Client::builder().build(https_connector);
    // Hyper expects an absolute `Uri` to allow it to know which server to connect too.
    // Currently, tonic's generated code only sets the `path_and_query` section so we
    // are going to write a custom tower layer in front of the hyper client to add the
    // scheme and authority.
    let uri = Uri::from_str(endpoint)?;
    let service = tower::ServiceBuilder::new()
        .map_request(move |mut req: http::Request<tonic::body::BoxBody>| {
            let uri = Uri::builder()
                .scheme(uri.scheme().expect("uri needs a scheme://").clone())
                .authority(
                    uri.authority()
                        .expect("uri needs an authority (host and port)")
                        .clone(),
                )
                .path_and_query(
                    req.uri()
                        .path_and_query()
                        .expect("uri needs a path")
                        .clone(),
                )
                .build()
                .expect("could not build uri");

            *req.uri_mut() = uri;
            req
        })
        .service(https_client)
        .boxed();

    Ok(Buffer::new(service, 1024))
}

struct StupidVerifier {}

impl ServerCertVerifier for StupidVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &tokio_rustls::rustls::Certificate,
        _intermediates: &[tokio_rustls::rustls::Certificate],
        _server_name: &tokio_rustls::rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<tokio_rustls::rustls::client::ServerCertVerified, tokio_rustls::rustls::Error> {
        // roflmao
        Ok(tokio_rustls::rustls::client::ServerCertVerified::assertion())
    }
}
