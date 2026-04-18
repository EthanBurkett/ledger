//! `ledger cat <hash>` — fetch a blob and write its bytes to stdout
//! (or `--out <file>`).

use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use clap::{Arg, ArgMatches, Command};
use clap_action_command::vec1::Vec1;
use serde::Deserialize;

use super::ActionCommand;
use crate::cli::client::Client;

#[derive(Debug, Deserialize)]
struct BlobPayload {
    #[allow(dead_code)]
    hash: String,
    #[allow(dead_code)]
    size: i64,
    content_base64: String,
}

pub struct CatCommand {}

impl ActionCommand for CatCommand {
    fn name(&self) -> &'static str {
        "cat"
    }

    fn command(&self, command: Command) -> Command {
        command
            .about("Fetch a blob by hash and write its bytes")
            .arg(
                Arg::new("hash")
                    .required(true)
                    .value_name("BLOB_HASH"),
            )
            .arg(
                Arg::new("out")
                    .long("out")
                    .short('o')
                    .value_name("PATH")
                    .help("Write to file instead of stdout"),
            )
    }

    fn action(&self, matches: Vec1<&ArgMatches>) -> Result<(), Box<dyn Error>> {
        let leaf = matches.last();
        let hash = leaf
            .get_one::<String>("hash")
            .cloned()
            .ok_or("hash is required")?;
        let out = leaf.get_one::<String>("out").map(PathBuf::from);

        let mut client = Client::authed()?;
        let blob: BlobPayload = client.get(&format!("/v1/blobs/{hash}"))?;
        let bytes = B64
            .decode(blob.content_base64.as_bytes())
            .map_err(|e| format!("server returned invalid base64: {e}"))?;

        match out {
            Some(path) => {
                fs::write(&path, &bytes)?;
                eprintln!("wrote {} bytes to {}", bytes.len(), path.display());
            }
            None => {
                std::io::stdout().write_all(&bytes)?;
            }
        }
        Ok(())
    }
}
