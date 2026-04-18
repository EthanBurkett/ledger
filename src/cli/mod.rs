use clap::Command;
use clap_action_command::vec1::vec1;
use clap_action_command::CommandMap;

pub mod client;
pub mod commands;
pub mod config;
pub mod workdir;

pub fn init() {
    let command_map = CommandMap::builder()
        // Auth / session.
        .push(commands::register::RegisterCommand {})
        .push(commands::login::LoginCommand {})
        .push(commands::logout::LogoutCommand {})
        .push(commands::whoami::WhoamiCommand {})
        // Repo lifecycle.
        .push(commands::init::InitCommand {})
        .push(commands::repos::ReposCommand {})
        // Workflow.
        .push(commands::add::AddCommand {})
        .push(commands::status::StatusCommand {})
        .push(commands::reset::ResetCommand {})
        .push(commands::commit::CommitCommand {})
        .push(commands::log::LogCommand {})
        .push(commands::diff::DiffCommand {})
        // Refs / navigation.
        .push(commands::branch::BranchCommand {})
        .push(commands::checkout::CheckoutCommand {})
        .push(commands::cat::CatCommand {})
        // Server + scratch.
        .push(commands::start::StartCommand {})
        .push(commands::test::TestCommand {})
        .build();

    let command = Command::new("ledger")
        .disable_help_subcommand(true)
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommands(command_map.commands());
    let matches = command.get_matches();

    if let Err(err) = command_map.dispatch(vec1![&matches]) {
        // `DispatchError::ActionCommand` wraps the real error as `source`.
        // Its `Display` is generic ("an error in the business logic"), so
        // walk the chain to surface what the command actually said.
        let mut current: Option<&dyn std::error::Error> = Some(&err);
        let mut first = true;
        while let Some(e) = current {
            if first {
                eprintln!("error: {e}");
                first = false;
            } else {
                eprintln!("  caused by: {e}");
            }
            current = e.source();
        }
        std::process::exit(1);
    }
}
