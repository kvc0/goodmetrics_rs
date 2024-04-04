use std::{str::FromStr, sync::Arc};

use hyper::{
    client::HttpConnector,
    header::HeaderName,
    http::{self, HeaderValue},
    Body, Error, Request, Response, Uri,
};
use tokio_rustls::rustls::{client::ServerCertVerifier, ClientConfig, RootCertStore};
use tonic::body::BoxBody;
use tower::{buffer::Buffer, util::BoxService, ServiceExt};

use super::StdError;

pub type ChannelType =
    Buffer<BoxService<Request<BoxBody>, Response<Body>, Error>, Request<BoxBody>>;

/// You can make an insecure connection by passing `|| { None }` to tls_trust.
/// If you want to make a safer connection you can add your trust roots,
/// for example:
/// ```rust
///  || {
///     let mut store = tokio_rustls::rustls::RootCertStore::empty();
///     store.add_server_trust_anchors(
///         webpki_roots::TLS_SERVER_ROOTS.iter().map(|trust_anchor| {
///             tokio_rustls::rustls::OwnedTrustAnchor::from_subject_spki_name_constraints(
///                 trust_anchor.subject.to_vec(),
///                 trust_anchor.subject_public_key_info.to_vec(),
///                 trust_anchor.name_constraints.as_ref().map(|der| der.to_vec())
///             )
///         })
///     );
///     Some(store)
/// }
/// ;
/// ```
pub fn get_channel<TrustFunction>(
    endpoint: &str,
    tls_trust: TrustFunction,
    header: Option<(HeaderName, HeaderValue)>,
) -> Result<ChannelType, StdError>
where
    TrustFunction: FnOnce() -> Option<RootCertStore>,
{
    let tls = ClientConfig::builder().with_safe_defaults();
    let tls = match tls_trust() {
        Some(trust) => tls.with_root_certificates(trust).with_no_client_auth(),
        None => {
            let mut config = tls
                .with_root_certificates(RootCertStore::empty())
                .with_no_client_auth();
            config
                .dangerous()
                .set_certificate_verifier(Arc::new(StupidVerifier {}));
            config
        }
    };

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
            match &header {
                Some((name, value)) => {
                    req.headers_mut().append(name, value.clone());
                }
                None => {}
            };

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
