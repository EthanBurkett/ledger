use std::error::Error;
use clap::{ArgMatches, Command};
use clap_action_command::vec1::Vec1;
use super::ActionCommand;

pub struct TestCommand {}

impl ActionCommand for TestCommand {
    fn name(&self) -> &'static str {
        "test"
    }

    fn command(&self, command: Command) -> Command {
        command.about("Testing 123")
            .alias("t")
    }

    fn action(&self, _matches: Vec1<&ArgMatches>) -> Result<(), Box<dyn Error>> {
        println!("it's da test command!");

        Ok(())
    }
}