use std::{str::FromStr, sync::Arc};

use hyper::Uri;
use hyper_util::{client::legacy::connect::HttpConnector, rt::TokioExecutor};
use tokio_rustls::rustls::{
    client::danger::ServerCertVerifier, crypto::aws_lc_rs, ClientConfig, RootCertStore,
};

use super::StdError;

/// Type alias for internal channel type
pub type ChannelType = hyper_util::client::legacy::Client<
    hyper_rustls::HttpsConnector<HttpConnector>,
    tonic::body::BoxBody,
>;

/// You can make an insecure connection by passing `|| { None }` to tls_trust.
/// If you want to make a safer connection you can add your trust roots,
/// for example:
/// ```rust
/// || {
///     Some(tokio_rustls::rustls::RootCertStore {
///         roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
///     })
/// };
/// ```
pub fn get_client<TrustFunction, WithOrigin, U>(
    endpoint: &str,
    tls_trust: TrustFunction,
    with_origin: WithOrigin,
) -> Result<U, StdError>
where
    TrustFunction: FnOnce() -> Option<RootCertStore>,
    WithOrigin: Fn(ChannelType, Uri) -> U,
{
    let tls = ClientConfig::builder_with_provider(Arc::new(aws_lc_rs::default_provider()))
        .with_safe_default_protocol_versions()?;
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

    let https_client = hyper_util::client::legacy::Client::builder(TokioExecutor::new())
        .http2_only(true)
        .build(https_connector);
    let uri = Uri::from_str(endpoint)?;

    // Using `with_origin` will let the codegenerated client set the `scheme` and
    // `authority` from the provided `Uri`. You need to pass "https://example.com"

    Ok(with_origin(https_client, uri))
}

#[derive(Debug)]
struct StupidVerifier {}

impl ServerCertVerifier for StupidVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &tonic::transport::CertificateDer<'_>,
        _intermediates: &[tonic::transport::CertificateDer<'_>],
        _server_name: &tokio_rustls::rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: tokio_rustls::rustls::pki_types::UnixTime,
    ) -> Result<tokio_rustls::rustls::client::danger::ServerCertVerified, tokio_rustls::rustls::Error>
    {
        // roflmao
        Ok(tokio_rustls::rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &tonic::transport::CertificateDer<'_>,
        _dss: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> Result<
        tokio_rustls::rustls::client::danger::HandshakeSignatureValid,
        tokio_rustls::rustls::Error,
    > {
        // roflmao
        Ok(tokio_rustls::rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &tonic::transport::CertificateDer<'_>,
        _dss: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> Result<
        tokio_rustls::rustls::client::danger::HandshakeSignatureValid,
        tokio_rustls::rustls::Error,
    > {
        // roflmao
        Ok(tokio_rustls::rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<tokio_rustls::rustls::SignatureScheme> {
        tokio_rustls::rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
