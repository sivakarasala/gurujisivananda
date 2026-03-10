use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region, RequestChecksumCalculation};

use crate::configuration::S3Settings;

pub async fn build_s3_client(settings: &S3Settings) -> S3Client {
    let credentials = Credentials::new(
        &settings.access_key,
        &settings.secret_key,
        None,
        None,
        "do-spaces",
    );

    // Build an HTTP/1.1-only connector to avoid HTTP/2 PROTOCOL_ERROR
    // with DigitalOcean Spaces. The default SDK connector negotiates HTTP/2
    // via TLS ALPN, which DO Spaces doesn't handle correctly for uploads.
    let https_connector = hyper_rustls_0_24::HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_or_http()
        .enable_http1()
        .build();

    #[allow(deprecated)]
    let http_client = aws_smithy_http_client::hyper_014::HyperClientBuilder::new()
        .build(https_connector);

    let config = aws_sdk_s3::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .region(Region::new(settings.region.clone()))
        .endpoint_url(&settings.endpoint)
        .credentials_provider(credentials)
        .force_path_style(true)
        .request_checksum_calculation(RequestChecksumCalculation::WhenRequired)
        .http_client(http_client)
        .build();

    S3Client::from_conf(config)
}
