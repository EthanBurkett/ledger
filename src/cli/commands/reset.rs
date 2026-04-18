//! `ledger reset [path]` — unstage a single path, or drop everything.

use std::error::Error;
use std::path::PathBuf;

use clap::{Arg, ArgAction, ArgMatches, Command};
use clap_action_command::vec1::Vec1;
use serde_json::Value;

use super::ActionCommand;
use crate::cli::client::Client;
use crate::cli::workdir;

pub struct ResetCommand {}

impl ActionCommand for ResetCommand {
    fn name(&self) -> &'static str {
        "reset"
    }

    fn command(&self, command: Command) -> Command {
        command
            .about("Unstage changes. With a PATH, unstages that entry; with --all, clears the index.")
            .arg(
                Arg::new("path")
                    .value_name("PATH")
                    .conflicts_with("all")
                    .help("Path to unstage (must live inside the workdir)"),
            )
            .arg(
                Arg::new("all")
                    .long("all")
                    .action(ArgAction::SetTrue)
                    .help("Clear the entire staging index"),
            )
    }

    fn action(&self, matches: Vec1<&ArgMatches>) -> Result<(), Box<dyn Error>> {
        let leaf = matches.last();
        let wd = workdir::require()?;
        let mut client = Client::authed()?;

        if leaf.get_flag("all") {
            let _: Value = client.delete(&format!(
                "/v1/repos/{}/index/all",
                wd.config.repo_id
            ))?;
            println!("cleared staging index");
            return Ok(());
        }

        let path = leaf
            .get_one::<String>("path")
            .cloned()
            .ok_or("either PATH or --all is required")?;
        let rel = wd.posix_relative(&PathBuf::from(&path))?;
        let encoded = urlencoding_encode(&rel);
        let _: Value = client.delete(&format!(
            "/v1/repos/{}/index?path={}",
            wd.config.repo_id, encoded
        ))?;
        println!("unstaged {rel}");
        Ok(())
    }
}

/// Very small percent-encoder for query values. Covers the characters that
/// matter for file paths on every platform we target.
fn urlencoding_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
