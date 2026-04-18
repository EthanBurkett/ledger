//! `ledger login <username>` — authenticate and cache tokens locally.

use std::error::Error;

use clap::{Arg, ArgAction, ArgMatches, Command};
use clap_action_command::vec1::Vec1;

use super::ActionCommand;
use crate::cli::client::{save_session, Client, SessionView};

pub struct LoginCommand {}

impl ActionCommand for LoginCommand {
    fn name(&self) -> &'static str {
        "login"
    }

    fn command(&self, command: Command) -> Command {
        command
            .about("Log in and cache tokens for subsequent commands")
            .arg(
                Arg::new("username")
                    .required(true)
                    .value_name("USERNAME"),
            )
            .arg(
                Arg::new("password")
                    .long("password")
                    .short('p')
                    .env("LEDGER_PASSWORD")
                    .required(true)
                    .value_name("PASSWORD"),
            )
            .arg(
                Arg::new("stay-logged-in")
                    .long("stay-logged-in")
                    .action(ArgAction::SetTrue)
                    .help("Request a longer-lived token pair"),
            )
    }

    fn action(&self, matches: Vec1<&ArgMatches>) -> Result<(), Box<dyn Error>> {
        let leaf = matches.last();
        let username = leaf
            .get_one::<String>("username")
            .cloned()
            .ok_or("username is required")?;
        let password = leaf
            .get_one::<String>("password")
            .cloned()
            .ok_or("password is required")?;
        let stay = leaf.get_flag("stay-logged-in");

        let mut client = Client::anonymous()?;
        let session: SessionView = client.post(
            "/v1/auth/login",
            &serde_json::json!({
                "username": username,
                "password": password,
                "stay_logged_in": stay,
            }),
        )?;

        let api_url = crate::cli::config::api_url();
        save_session(&api_url, &session.user.username, &session.tokens)?;
        println!("logged in as {}", session.user.username);
        if stay {
            println!("  stay-logged-in session (extended TTLs)");
        }
        println!("  api url:   {api_url}");
        Ok(())
    }
}
