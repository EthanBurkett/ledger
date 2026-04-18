//! `ledger status` — print the repo binding and every staged entry.

use std::error::Error;

use clap::{ArgMatches, Command};
use clap_action_command::vec1::Vec1;
use serde::Deserialize;

use super::ActionCommand;
use crate::cli::client::Client;
use crate::cli::workdir;

#[derive(Debug, Deserialize)]
struct IndexEntry {
    path: String,
    blob_hash: String,
}

#[derive(Debug, Deserialize)]
struct IndexView {
    #[allow(dead_code)]
    id: Option<String>,
    #[allow(dead_code)]
    repo_id: String,
    entries: Vec<IndexEntry>,
}

#[derive(Debug, Deserialize)]
struct RepoView {
    id: String,
    name: String,
    head_commit: Option<String>,
    #[allow(dead_code)]
    owner_id: String,
}

pub struct StatusCommand {}

impl ActionCommand for StatusCommand {
    fn name(&self) -> &'static str {
        "status"
    }

    fn command(&self, command: Command) -> Command {
        command
            .about("Show the current repo, HEAD, and staged changes")
            .alias("st")
    }

    fn action(&self, _matches: Vec1<&ArgMatches>) -> Result<(), Box<dyn Error>> {
        let wd = workdir::require()?;
        let mut client = Client::authed()?;

        let repo: RepoView = client.get(&format!("/v1/repos/{}", wd.config.repo_id))?;
        println!("repo:   {} ({})", repo.name, repo.id);
        println!("workdir: {}", wd.root.display());
        println!(
            "head:   {}",
            repo.head_commit.as_deref().unwrap_or("(none)")
        );

        let idx: IndexView =
            client.get(&format!("/v1/repos/{}/index", wd.config.repo_id))?;
        if idx.entries.is_empty() {
            println!("\nnothing staged");
        } else {
            println!("\nstaged ({}):", idx.entries.len());
            for e in idx.entries {
                println!("  {}  {}", short_hash(&e.blob_hash), e.path);
            }
        }
        Ok(())
    }
}

fn short_hash(h: &str) -> String {
    if h.len() > 12 {
        h[..12].to_string()
    } else {
        h.to_string()
    }
}
