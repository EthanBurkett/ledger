//! `ledger register <username>` — create a new account via the API.

use std::error::Error;

use clap::{Arg, ArgMatches, Command};
use clap_action_command::vec1::Vec1;

use super::ActionCommand;
use crate::cli::client::{save_session, Client, SessionView};

pub struct RegisterCommand {}

impl ActionCommand for RegisterCommand {
    fn name(&self) -> &'static str {
        "register"
    }

    fn command(&self, command: Command) -> Command {
        command
            .about("Create a new account and log in")
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
                    .value_name("PASSWORD")
                    .help("Plaintext password (sent over HTTPS; hashed by the server)"),
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
            .ok_or("password is required (use --password or LEDGER_PASSWORD)")?;

        let mut client = Client::anonymous()?;
        let session: SessionView = client.post(
            "/v1/auth/register",
            &serde_json::json!({
                "username": username,
                "password": password,
            }),
        )?;

        let api_url = crate::cli::config::api_url();
        save_session(&api_url, &session.user.username, &session.tokens)?;
        println!("registered and logged in as {}", session.user.username);
        println!("  user id:   {}", session.user.id);
        println!("  api url:   {api_url}");
        Ok(())
    }
}
