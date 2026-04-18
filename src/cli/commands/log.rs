//! `ledger log` — show the history reachable from HEAD.

use std::error::Error;

use clap::{Arg, ArgMatches, Command};
use clap_action_command::vec1::Vec1;
use serde::Deserialize;

use super::ActionCommand;
use crate::cli::client::Client;
use crate::cli::workdir;

#[derive(Debug, Deserialize)]
struct RepoView {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    name: String,
    head_commit: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CommitView {
    hash: String,
    #[allow(dead_code)]
    tree: String,
    #[allow(dead_code)]
    parents: Vec<String>,
    message: String,
    timestamp: i64,
}

pub struct LogCommand {}

impl ActionCommand for LogCommand {
    fn name(&self) -> &'static str {
        "log"
    }

    fn command(&self, command: Command) -> Command {
        command
            .about("Show commit history reachable from HEAD")
            .arg(
                Arg::new("limit")
                    .long("limit")
                    .short('n')
                    .default_value("20")
                    .value_name("N")
                    .help("Maximum number of commits to print"),
            )
            .arg(
                Arg::new("from")
                    .long("from")
                    .value_name("COMMIT_HASH")
                    .help("Start history from this commit instead of HEAD"),
            )
    }

    fn action(&self, matches: Vec1<&ArgMatches>) -> Result<(), Box<dyn Error>> {
        let leaf = matches.last();
        let limit: usize = leaf
            .get_one::<String>("limit")
            .and_then(|s| s.parse().ok())
            .unwrap_or(20);
        let from = leaf.get_one::<String>("from").cloned();

        let wd = workdir::require()?;
        let mut client = Client::authed()?;

        let head = match from {
            Some(h) => h,
            None => {
                let repo: RepoView =
                    client.get(&format!("/v1/repos/{}", wd.config.repo_id))?;
                match repo.head_commit {
                    Some(h) => h,
                    None => {
                        println!("no commits yet");
                        return Ok(());
                    }
                }
            }
        };

        let commits: Vec<CommitView> = client.get(&format!(
            "/v1/commits/{head}/history?limit={limit}"
        ))?;

        if commits.is_empty() {
            println!("no commits");
            return Ok(());
        }

        for c in commits {
            println!("commit {}", c.hash);
            println!("  timestamp: {}", c.timestamp);
            println!();
            for line in c.message.lines() {
                println!("    {line}");
            }
            println!();
        }
        Ok(())
    }
}
