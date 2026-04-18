//! `ledger whoami` — print the authenticated user (hits `/v1/auth/me`).

use std::error::Error;

use clap::{ArgMatches, Command};
use clap_action_command::vec1::Vec1;

use super::ActionCommand;
use crate::cli::client::{Client, UserView};

pub struct WhoamiCommand {}

impl ActionCommand for WhoamiCommand {
    fn name(&self) -> &'static str {
        "whoami"
    }

    fn command(&self, command: Command) -> Command {
        command.about("Show the currently authenticated user")
    }

    fn action(&self, _matches: Vec1<&ArgMatches>) -> Result<(), Box<dyn Error>> {
        let mut client = Client::authed()?;
        let me: UserView = client.get("/v1/auth/me")?;
        println!("{}", me.username);
        println!("  id:            {}", me.id);
        println!("  created_at:    {}", me.created_at);
        match me.last_login_at {
            Some(t) => println!("  last_login_at: {t}"),
            None => println!("  last_login_at: -"),
        }
        if let Some(creds) = client.credentials() {
            println!("  api:           {}", creds.api_url);
        }
        Ok(())
    }
}
