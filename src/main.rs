mod cli;
mod runner;

use clap::{CommandFactory, FromArgMatches};

fn main() {
    let cmd = cli::Cli::command().styles(cli::make_styles());
    let parser = cmd.get_matches();
    let arg = cli::Cli::from_arg_matches(&parser).unwrap();

    let runner = runner::Runner::new(arg.source, arg.input, arg.output, arg.max_chars);
    runner.run();
}
