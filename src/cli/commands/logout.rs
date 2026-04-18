//! `ledger logout` — revoke the refresh token server-side and delete
//! local credentials.

use std::error::Error;

use clap::{ArgMatches, Command};
use clap_action_command::vec1::Vec1;
use serde_json::Value;

use super::ActionCommand;
use crate::cli::client::Client;
use crate::cli::config::Credentials;

pub struct LogoutCommand {}

impl ActionCommand for LogoutCommand {
    fn name(&self) -> &'static str {
        "logout"
    }

    fn command(&self, command: Command) -> Command {
        command.about("Log out and clear cached credentials")
    }

    fn action(&self, _matches: Vec1<&ArgMatches>) -> Result<(), Box<dyn Error>> {
        let Some(creds) = Credentials::load_ok() else {
            println!("no active session");
            return Ok(());
        };

        // Best-effort server-side revoke; still wipe local creds on failure.
        match Client::authed() {
            Ok(mut client) => {
                let body = serde_json::json!({ "refresh_token": creds.refresh_token });
                let _: Result<Value, _> = client.post("/v1/auth/logout", &body);
            }
            Err(e) => eprintln!("warning: could not reach server to revoke token: {e}"),
        }

        let removed = Credentials::clear()?;
        if removed {
            println!("logged out ({})", creds.username);
        } else {
            println!("already logged out");
        }
        Ok(())
    }
}
