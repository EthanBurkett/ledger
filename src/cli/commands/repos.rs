//! `ledger repos` — list repositories owned by the authenticated user.

use std::error::Error;

use clap::{ArgMatches, Command};
use clap_action_command::vec1::Vec1;
use serde::Deserialize;

use super::ActionCommand;
use crate::cli::client::Client;

#[derive(Debug, Deserialize)]
struct RepoView {
    id: String,
    #[allow(dead_code)]
    owner_id: String,
    name: String,
    head_commit: Option<String>,
}

pub struct ReposCommand {}

impl ActionCommand for ReposCommand {
    fn name(&self) -> &'static str {
        "repos"
    }

    fn command(&self, command: Command) -> Command {
        command
            .about("List repositories you own")
            .alias("ls")
    }

    fn action(&self, _matches: Vec1<&ArgMatches>) -> Result<(), Box<dyn Error>> {
        let mut client = Client::authed()?;
        let repos: Vec<RepoView> = client.get("/v1/repos")?;
        if repos.is_empty() {
            println!("no repos yet; run `ledger init <name>`");
            return Ok(());
        }
        println!("{:<26} {:<40} {}", "id", "name", "head");
        for r in repos {
            println!(
                "{:<26} {:<40} {}",
                r.id,
                r.name,
                r.head_commit.as_deref().unwrap_or("-")
            );
        }
        Ok(())
    }
}
