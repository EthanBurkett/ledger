use clap::Command;
use clap_action_command::vec1::vec1;
use clap_action_command::CommandMap;

pub mod commands;

pub fn init() {
    let command_map = CommandMap::builder()
        .push(commands::test::TestCommand {})
        .push(commands::start::StartCommand {})
        .build();

    let command = Command::new("ledger")
        .disable_help_subcommand(true)
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommands(command_map.commands());
    let matches = command.get_matches();

    if let Err(err) = command_map.dispatch(vec1![&matches]) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
