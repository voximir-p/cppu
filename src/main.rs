mod cli;

#[path = "mod.rs"]
mod runner;

use clap::{CommandFactory, FromArgMatches};

fn main() {
    let mut command = cli::Cli::command().styles(cli::make_styles());

    if std::env::args_os().len() == 1 {
        let _ = command.print_long_help();
        println!();
        return;
    }

    let matches = command.get_matches();
    let args = cli::Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());

    std::process::exit(runner::Runner::new(args).run());
}
