use hyper_idp::server::{IdpCreds, IdpServer};
use object_api::server::ObjectApi;
use std::fs;
use storage::Storage;
use structopt::StructOpt;
use tokio::signal;
use tokio::sync::oneshot;
use tracing::{error, info};
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Debug, StructOpt)]
#[structopt(name = "object-store")]
struct Command {
    #[structopt(long, default_value = "443", env = "SSL_PORT")]
    ssl_port: u16,
    #[structopt(long, env = "SSL_CERT_PATH")]
    ssl_cert_path: String,
    #[structopt(long, env = "S3_ACCESS_KEY_ID")]
    s3_access_key_id: String,
    #[structopt(long, env = "S3_SECRET_ACCESS_KEY")]
    s3_secret_access_key: String,
    #[structopt(long, env = "S3_HOST", default_value = "https://minio.wavey.io:9000")]
    s3_host: String,
    #[structopt(long, env = "OIDC_AUDIENCE")]
    oidc_audience: String,
    #[structopt(long, env = "OIDC_CLIENT_ID")]
    oidc_client_id: String,
    #[structopt(long, env = "OIDC_CLIENT_SECRET")]
    oidc_client_secret: String,
    #[structopt(long, env = "OIDC_REDIRECT_URI")]
    oidc_redirect_uri: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    env_logger::init();

    let subscriber = fmt::Subscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .flatten_event(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("failed to set global default subscriber");

    let args = Command::from_args();
    let ssl_path = args.ssl_cert_path;
    let ssl_port = args.ssl_port;
    let s3_key = args.s3_access_key_id;
    let s3_secret = args.s3_secret_access_key;
    let s3_host = args.s3_host;

    let signing_cert: Vec<u8> = fs::read(format!("{}/{}", ssl_path, "auth0.pem")).unwrap();

    let oidc_creds = IdpCreds {
        audience: args.oidc_audience,
        client_id: args.oidc_client_id,
        client_secret: args.oidc_client_secret,
        redirect_uri: args.oidc_redirect_uri,
        signing_cert,
    };

    let idp_server = IdpServer::new(ssl_path.clone(), 8003, oidc_creds);
    let shutdown_idp = idp_server.start().await?;

    let storage_client = Storage::new(s3_host, s3_key, s3_secret, 1024 * 256);
    let object_api = ObjectApi::new(ssl_path.clone(), 8004, 8003, storage_client.clone());
    let shutdown_object_api = object_api.start().await?;

    let (tx, rx) = oneshot::channel();
    let handle = tokio::spawn(async move {
        if let Ok(()) = signal::ctrl_c().await {}
        let _ = tx.send(());
    });

    let _ = rx.await;

    shutdown_idp.send(()).unwrap();
    shutdown_object_api.send(()).unwrap();

    let _ = handle.await;

    Ok(())
}
