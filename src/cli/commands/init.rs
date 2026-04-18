//! `ledger init <name>` — create a repo server-side and bind the current
//! directory to it (writes `.ledger/config.json`).

use std::error::Error;

use clap::{Arg, ArgMatches, Command};
use clap_action_command::vec1::Vec1;
use serde::Deserialize;

use super::ActionCommand;
use crate::cli::client::Client;
use crate::cli::workdir::{self, WorkdirConfig};

#[derive(Debug, Deserialize)]
struct RepoView {
    id: String,
    #[allow(dead_code)]
    owner_id: String,
    name: String,
    #[allow(dead_code)]
    head_commit: Option<String>,
}

pub struct InitCommand {}

impl ActionCommand for InitCommand {
    fn name(&self) -> &'static str {
        "init"
    }

    fn command(&self, command: Command) -> Command {
        command
            .about("Create a repository and bind the current directory to it")
            .arg(
                Arg::new("name")
                    .required(true)
                    .value_name("NAME")
                    .help("Repository name (globally unique)"),
            )
    }

    fn action(&self, matches: Vec1<&ArgMatches>) -> Result<(), Box<dyn Error>> {
        let leaf = matches.last();
        let name = leaf
            .get_one::<String>("name")
            .cloned()
            .ok_or("name is required")?;

        let cwd = std::env::current_dir()?;
        if let Some(existing) = workdir::find_from(&cwd)? {
            return Err(format!(
                "{} already contains a .ledger workdir for repo {}",
                existing.root.display(),
                existing.config.repo_name
            )
            .into());
        }

        let mut client = Client::authed()?;
        let repo: RepoView = client.post("/v1/repos", &serde_json::json!({ "name": name }))?;

        let wd = workdir::init_at(
            &cwd,
            &WorkdirConfig {
                repo_id: repo.id.clone(),
                repo_name: repo.name.clone(),
                api_url: client.credentials().map(|c| c.api_url.clone()),
            },
        )?;

        println!("initialised repo {} ({})", repo.name, repo.id);
        println!("  workdir: {}", wd.dir().display());
        Ok(())
    }
}
