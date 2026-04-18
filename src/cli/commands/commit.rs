//! `ledger commit -m "..."` — turn the staging index into a commit.

use std::error::Error;

use clap::{Arg, ArgMatches, Command};
use clap_action_command::vec1::Vec1;
use serde::Deserialize;

use super::ActionCommand;
use crate::cli::client::Client;
use crate::cli::workdir;

#[derive(Debug, Deserialize)]
struct CommitView {
    hash: String,
    #[allow(dead_code)]
    repo_id: String,
    tree: String,
    parents: Vec<String>,
    message: String,
    #[allow(dead_code)]
    timestamp: i64,
}

pub struct CommitCommand {}

impl ActionCommand for CommitCommand {
    fn name(&self) -> &'static str {
        "commit"
    }

    fn command(&self, command: Command) -> Command {
        command
            .about("Create a commit from the current staging index")
            .alias("ci")
            .arg(
                Arg::new("message")
                    .long("message")
                    .short('m')
                    .required(true)
                    .value_name("MESSAGE")
                    .help("Commit message"),
            )
    }

    fn action(&self, matches: Vec1<&ArgMatches>) -> Result<(), Box<dyn Error>> {
        let leaf = matches.last();
        let message = leaf
            .get_one::<String>("message")
            .cloned()
            .ok_or("--message is required")?;

        let wd = workdir::require()?;
        let mut client = Client::authed()?;

        let commit: CommitView = client.post(
            &format!("/v1/repos/{}/index/commit", wd.config.repo_id),
            &serde_json::json!({ "message": message }),
        )?;

        println!("[{}] {}", short_hash(&commit.hash), commit.message);
        println!("  tree:    {}", commit.tree);
        if commit.parents.is_empty() {
            println!("  parents: (root commit)");
        } else {
            println!("  parents: {}", commit.parents.join(", "));
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
