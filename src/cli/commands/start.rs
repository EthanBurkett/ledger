use std::error::Error;

use clap::{Arg, ArgMatches, Command};
use clap_action_command::vec1::Vec1;

use super::ActionCommand;
use crate::db;

const DEFAULT_URI: &str = "mongodb://127.0.0.1:27017";
const DEFAULT_DB_NAME: &str = "ledger";

pub struct StartCommand {}

impl ActionCommand for StartCommand {
    fn name(&self) -> &'static str {
        "start"
    }

    fn command(&self, command: Command) -> Command {
        command
            .about("Starts the ledger service and keeps a MongoDB connection open")
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

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;

        runtime.block_on(async move { run(&uri, &db_name).await })
    }
}

async fn run(uri: &str, db_name: &str) -> Result<(), Box<dyn Error>> {
    println!("ledger: connecting to {uri} (database: {db_name})");
    let database = db::connect_and_sync(uri, Some(db_name)).await?;

    let models: Vec<&'static str> = db::registered_models()
        .map(|m| m.collection_name)
        .collect();
    println!(
        "ledger: synced {} model{} ({})",
        models.len(),
        if models.len() == 1 { "" } else { "s" },
        if models.is_empty() {
            "none registered".to_string()
        } else {
            models.join(", ")
        }
    );
    println!("ledger: ready — press Ctrl+C to stop");

    let _app = App { db: database };

    wait_for_shutdown().await?;

    println!("ledger: shutting down");
    Ok(())
}

#[allow(dead_code)]
pub struct App {
    pub db: db::Database,
}

#[cfg(unix)]
async fn wait_for_shutdown() -> Result<(), Box<dyn Error>> {
    use tokio::signal::unix::{signal, SignalKind};
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;
    tokio::select! {
        _ = sigint.recv() => {}
        _ = sigterm.recv() => {}
    }
    Ok(())
}

#[cfg(not(unix))]
async fn wait_for_shutdown() -> Result<(), Box<dyn Error>> {
    tokio::signal::ctrl_c().await?;
    Ok(())
}
