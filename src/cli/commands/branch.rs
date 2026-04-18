//! `ledger branch …` — manage named refs (branches).
//!
//! `ledger branch`                    list refs
//! `ledger branch create <name>`      point a new ref at HEAD (or `--commit <hash>`)
//! `ledger branch set <name> <hash>`  move an existing ref
//! `ledger branch delete <name>`      drop a ref

use std::error::Error;

use clap::{Arg, ArgMatches, Command};
use clap_action_command::vec1::Vec1;
use serde::Deserialize;
use serde_json::Value;

use super::ActionCommand;
use crate::cli::client::Client;
use crate::cli::workdir::{self, Workdir};

#[derive(Debug, Deserialize)]
struct RefView {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    repo_id: String,
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

pub struct BranchCommand {}

impl ActionCommand for BranchCommand {
    fn name(&self) -> &'static str {
        "branch"
    }

    fn command(&self, command: Command) -> Command {
        command
            .about("List, create, move, and delete named refs (branches)")
            .alias("b")
            .subcommand(Command::new("list").about("List refs").alias("ls"))
            .subcommand(
                Command::new("create")
                    .about("Point a new ref at a commit (defaults to HEAD)")
                    .arg(Arg::new("name").required(true).value_name("NAME"))
                    .arg(
                        Arg::new("commit")
                            .long("commit")
                            .value_name("COMMIT_HASH")
                            .help("Commit hash; defaults to the repo's HEAD"),
                    ),
            )
            .subcommand(
                Command::new("set")
                    .about("Move an existing ref to another commit")
                    .arg(Arg::new("name").required(true).value_name("NAME"))
                    .arg(
                        Arg::new("commit")
                            .required(true)
                            .value_name("COMMIT_HASH"),
                    ),
            )
            .subcommand(
                Command::new("delete")
                    .about("Delete a ref")
                    .alias("rm")
                    .arg(Arg::new("name").required(true).value_name("NAME")),
            )
    }

    fn action(&self, matches: Vec1<&ArgMatches>) -> Result<(), Box<dyn Error>> {
        let leaf = matches.last();
        let wd = workdir::require()?;
        match leaf.subcommand() {
            Some(("list", _)) | None => list(&wd),
            Some(("create", sub)) => create(&wd, sub),
            Some(("set", sub)) => set(&wd, sub),
            Some(("delete", sub)) => delete(&wd, sub),
            Some((other, _)) => Err(format!("unknown branch subcommand: {other}").into()),
        }
    }
}

fn list(wd: &Workdir) -> Result<(), Box<dyn Error>> {
    let mut client = Client::authed()?;
    let refs: Vec<RefView> =
        client.get(&format!("/v1/repos/{}/refs", wd.config.repo_id))?;
    if refs.is_empty() {
        println!("no branches");
        return Ok(());
    }
    for r in refs {
        println!("{:<24} {}", r.name, r.commit);
    }
    Ok(())
}

fn create(wd: &Workdir, matches: &ArgMatches) -> Result<(), Box<dyn Error>> {
    let name = matches
        .get_one::<String>("name")
        .cloned()
        .ok_or("name is required")?;
    let explicit_commit = matches.get_one::<String>("commit").cloned();

    let mut client = Client::authed()?;
    let commit = match explicit_commit {
        Some(c) => c,
        None => {
            let repo: RepoView =
                client.get(&format!("/v1/repos/{}", wd.config.repo_id))?;
            repo.head_commit
                .ok_or("repo has no HEAD yet; pass --commit <hash>")?
        }
    };

    let created: RefView = client.post(
        &format!("/v1/repos/{}/refs", wd.config.repo_id),
        &serde_json::json!({ "name": name, "commit": commit }),
    )?;
    println!("created {} → {}", created.name, created.commit);
    Ok(())
}

fn set(wd: &Workdir, matches: &ArgMatches) -> Result<(), Box<dyn Error>> {
    let name = matches
        .get_one::<String>("name")
        .cloned()
        .ok_or("name is required")?;
    let commit = matches
        .get_one::<String>("commit")
        .cloned()
        .ok_or("commit is required")?;

    let mut client = Client::authed()?;
    let updated: RefView = client.patch(
        &format!("/v1/repos/{}/refs/{}", wd.config.repo_id, name),
        &serde_json::json!({ "commit": commit }),
    )?;
    println!("moved {} → {}", updated.name, updated.commit);
    Ok(())
}

fn delete(wd: &Workdir, matches: &ArgMatches) -> Result<(), Box<dyn Error>> {
    let name = matches
        .get_one::<String>("name")
        .cloned()
        .ok_or("name is required")?;

    let mut client = Client::authed()?;
    let _: Value = client.delete(&format!(
        "/v1/repos/{}/refs/{}",
        wd.config.repo_id, name
    ))?;
    println!("deleted {name}");
    Ok(())
}
