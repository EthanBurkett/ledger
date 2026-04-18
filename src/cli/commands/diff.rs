//! `ledger diff <left> <right>` — compare two commits (or two trees).
//!
//! Defaults to comparing commits. Pass `--trees` to interpret the arguments
//! as tree hashes instead. Either side may be the empty string `""` to
//! represent an empty tree.

use std::error::Error;

use clap::{Arg, ArgAction, ArgMatches, Command};
use clap_action_command::vec1::Vec1;
use serde::Deserialize;

use super::ActionCommand;
use crate::cli::client::Client;

#[derive(Debug, Deserialize)]
struct Change {
    path: String,
    kind: String,
    old_hash: Option<String>,
    new_hash: Option<String>,
}

pub struct DiffCommand {}

impl ActionCommand for DiffCommand {
    fn name(&self) -> &'static str {
        "diff"
    }

    fn command(&self, command: Command) -> Command {
        command
            .about("Diff two commits (default) or two trees (--trees)")
            .arg(
                Arg::new("left")
                    .required(true)
                    .value_name("LEFT")
                    .help("Base side (use \"\" for empty)"),
            )
            .arg(
                Arg::new("right")
                    .required(true)
                    .value_name("RIGHT")
                    .help("New side (use \"\" for empty)"),
            )
            .arg(
                Arg::new("trees")
                    .long("trees")
                    .action(ArgAction::SetTrue)
                    .help("Treat LEFT/RIGHT as tree hashes instead of commits"),
            )
    }

    fn action(&self, matches: Vec1<&ArgMatches>) -> Result<(), Box<dyn Error>> {
        let leaf = matches.last();
        let left = leaf
            .get_one::<String>("left")
            .cloned()
            .unwrap_or_default();
        let right = leaf
            .get_one::<String>("right")
            .cloned()
            .unwrap_or_default();
        let use_trees = leaf.get_flag("trees");

        let mut client = Client::authed()?;
        let path = if use_trees { "trees" } else { "commits" };
        let url = format!(
            "/v1/diff/{path}?left={}&right={}",
            urlencode(&left),
            urlencode(&right)
        );
        let changes: Vec<Change> = client.get(&url)?;

        if changes.is_empty() {
            println!("no differences");
            return Ok(());
        }
        for c in changes {
            let marker = match c.kind.as_str() {
                "added" => '+',
                "deleted" => '-',
                "modified" => '~',
                _ => '?',
            };
            match (c.old_hash.as_deref(), c.new_hash.as_deref()) {
                (Some(old), Some(new)) => {
                    println!("{marker} {}  {} → {}", c.path, short(old), short(new));
                }
                (None, Some(new)) => println!("{marker} {}  {}", c.path, short(new)),
                (Some(old), None) => println!("{marker} {}  {}", c.path, short(old)),
                _ => println!("{marker} {}", c.path),
            }
        }
        Ok(())
    }
}

fn short(h: &str) -> String {
    if h.len() > 12 {
        h[..12].to_string()
    } else {
        h.to_string()
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
