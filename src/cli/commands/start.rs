use std::error::Error;
use std::net::SocketAddr;

use clap::{Arg, ArgMatches, Command};
use clap_action_command::vec1::Vec1;

use super::ActionCommand;
use crate::{api, app::App, auth::AuthConfig, db};

const DEFAULT_URI: &str = "mongodb://127.0.0.1:27017";
const DEFAULT_DB_NAME: &str = "ledger";
// NOTE: 8080 is frequently inside the Windows Hyper-V / WSL excluded port
// range (os error 10013). 3030 tends to sit outside it.
const DEFAULT_HTTP_ADDR: &str = "127.0.0.1:3030";

pub struct StartCommand {}

impl ActionCommand for StartCommand {
    fn name(&self) -> &'static str {
        "start"
    }

    fn command(&self, command: Command) -> Command {
        command
            .about("Starts the ledger service (MongoDB + HTTP API)")
            .alias("s")
            .arg(
                Arg::new("uri")
                    .long("uri")
                    .env("LEDGER_MONGO_URI")
                    .value_name("MONGO_URI")
                    .default_value(DEFAULT_URI)
                    .help("MongoDB connection string"),
            )
            .arg(
                Arg::new("database")
                    .long("database")
                    .short('d')
                    .env("LEDGER_DB_NAME")
                    .value_name("NAME")
                    .default_value(DEFAULT_DB_NAME)
                    .help("Database name to use"),
            )
            .arg(
                Arg::new("http-addr")
                    .long("http-addr")
                    .env("LEDGER_HTTP_ADDR")
                    .value_name("ADDR")
                    .default_value(DEFAULT_HTTP_ADDR)
                    .help("HTTP API bind address (host:port)"),
            )
    }

    fn action(&self, matches: Vec1<&ArgMatches>) -> Result<(), Box<dyn Error>> {
        let leaf = matches.last();
        let uri = leaf
            .get_one::<String>("uri")
            .cloned()
            .unwrap_or_else(|| DEFAULT_URI.to_string());
        let db_name = leaf
            .get_one::<String>("database")
            .cloned()
            .unwrap_or_else(|| DEFAULT_DB_NAME.to_string());
        let http_addr: SocketAddr = leaf
            .get_one::<String>("http-addr")
            .cloned()
            .unwrap_or_else(|| DEFAULT_HTTP_ADDR.to_string())
            .parse()
            .map_err(|e| format!("invalid --http-addr: {e}"))?;

        init_tracing();

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;

        runtime.block_on(async move {
            match run(&uri, &db_name, http_addr).await {
                Ok(()) => Ok(()),
                Err(e) => {
                    tracing::error!(error = %e, "ledger: fatal");
                    eprintln!("ledger: fatal: {e}");
                    let mut source = e.source();
                    while let Some(s) = source {
                        eprintln!("  caused by: {s}");
                        source = s.source();
                    }
                    Err(e)
                }
            }
        })
    }
}

fn init_tracing() {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    let filter = EnvFilter::try_from_env("LEDGER_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info"));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false).compact())
        .try_init();
}

async fn run(uri: &str, db_name: &str, http_addr: SocketAddr) -> Result<(), Box<dyn Error>> {
    if let Some(path) = crate::env::loaded_path() {
        tracing::info!(env_file = %path.display(), "ledger: loaded .env");
    }

    tracing::info!(uri, db_name, "ledger: connecting to MongoDB");
    let database = db::connect_and_sync(uri, Some(db_name)).await?;
    App::init(database, AuthConfig::from_env())?;

    let models: Vec<&'static str> = db::registered_models()
        .map(|m| m.collection_name)
        .collect();
    tracing::info!(
        count = models.len(),
        collections = %models.join(", "),
        "ledger: models synced"
    );

    let routes: Vec<&'static str> = api::registered_modules().map(|m| m.name).collect();
    tracing::info!(
        count = routes.len(),
        modules = %routes.join(", "),
        "ledger: routes registered"
    );

    if let Err(e) = api::serve(http_addr, shutdown_signal()).await {
        tracing::error!(error = %e, addr = %http_addr, "ledger: http server failed");
        let msg = e.to_string();
        if msg.contains("10013") {
            eprintln!(
                "\nhint: port {port} is inside a Windows reserved port range.\n\
                 run `netsh interface ipv4 show excludedportrange protocol=tcp`\n\
                 to see the reserved ranges, or pass --http-addr 127.0.0.1:<PORT>\n\
                 with a port outside them.\n",
                port = http_addr.port()
            );
        } else if msg.contains("10048") || msg.to_lowercase().contains("address in use") {
            eprintln!(
                "\nhint: {addr} is already in use. pick another port with --http-addr.\n",
                addr = http_addr
            );
        }
        return Err(e);
    }

    tracing::info!("ledger: shut down cleanly");
    Ok(())
}

#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let sigint = async {
        if let Ok(mut s) = signal(SignalKind::interrupt()) {
            s.recv().await;
        }
    };
    let sigterm = async {
        if let Ok(mut s) = signal(SignalKind::terminate()) {
            s.recv().await;
        }
    };
    tokio::select! {
        _ = sigint => {}
        _ = sigterm => {}
    }
    tracing::info!("ledger: shutdown signal received");
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("ledger: shutdown signal received");
}
