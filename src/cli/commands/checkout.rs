//! `ledger checkout <ref-or-commit>` — move HEAD.
//!
//! If the argument matches a branch name we resolve it through
//! `/v1/repos/{id}/refs/{name}`; otherwise we treat it as a literal commit
//! hash. Either way we call `PUT /v1/repos/{id}/head`.

use std::error::Error;

use clap::{Arg, ArgMatches, Command};
use clap_action_command::vec1::Vec1;
use serde::Deserialize;

use super::ActionCommand;
use crate::cli::client::{CliError, Client};
use crate::cli::workdir;

#[derive(Debug, Deserialize)]
struct RefView {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    repo_id: String,
    #[allow(dead_code)]
    name: String,
    commit: String,
}

#[derive(Debug, Deserialize)]
struct RepoView {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    name: String,
    head_commit: Option<String>,
}

pub struct CheckoutCommand {}

impl ActionCommand for CheckoutCommand {
    fn name(&self) -> &'static str {
        "checkout"
    }

    fn command(&self, command: Command) -> Command {
        command
            .about("Move HEAD to a branch name or commit hash")
            .alias("co")
            .arg(
                Arg::new("target")
                    .required(true)
                    .value_name("REF_OR_COMMIT"),
            )
    }

    fn action(&self, matches: Vec1<&ArgMatches>) -> Result<(), Box<dyn Error>> {
        let leaf = matches.last();
        let target = leaf
            .get_one::<String>("target")
            .cloned()
            .ok_or("REF_OR_COMMIT is required")?;

        let wd = workdir::require()?;
        let mut client = Client::authed()?;

        // Try to resolve as a ref first. 404 → fall through to commit hash.
        let commit = match client.get::<RefView>(&format!(
            "/v1/repos/{}/refs/{}",
            wd.config.repo_id, target
        )) {
            Ok(r) => r.commit,
            Err(CliError::Api { status: 404, .. }) => target.clone(),
            Err(e) => return Err(Box::new(e)),
        };

        let repo: RepoView = client.put(
            &format!("/v1/repos/{}/head", wd.config.repo_id),
            &serde_json::json!({ "commit": commit }),
        )?;

        println!(
            "HEAD → {}",
            repo.head_commit.as_deref().unwrap_or("(cleared)")
        );
        Ok(())
    }
}
