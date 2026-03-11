mod cli;
mod runner;

use clap::{CommandFactory, FromArgMatches};
use std::env;
use std::process;

fn main() {
    let mut cmd = cli::Cli::command().styles(cli::make_styles());

    // Match --help behavior when invoked without arguments.
    if env::args_os().len() == 1 {
        let _ = cmd.print_long_help();
        println!();
        process::exit(0);
    }

    let parser = cmd.get_matches();
    let args = cli::Cli::from_arg_matches(&parser).unwrap();

    let runner = runner::Runner::new(args);
    let rc = runner.run();
    process::exit(rc);
}
