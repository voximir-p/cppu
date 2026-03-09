use anstyle::{AnsiColor, Color, Style};
use clap::builder::Styles as ClapStyles;
use std::path::PathBuf;

const MAX_CHARS: i64 = 50_000;

pub(crate) fn make_styles() -> ClapStyles {
    ClapStyles::styled()
        .header(
            Style::new()
                .bold()
                .fg_color(Some(Color::Ansi(AnsiColor::Green))),
        )
        .usage(
            Style::new()
                .bold()
                .fg_color(Some(Color::Ansi(AnsiColor::Green))),
        )
        .literal(
            Style::new()
                .bold()
                .fg_color(Some(Color::Ansi(AnsiColor::Cyan))),
        )
        .placeholder(Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan))))
        .error(
            Style::new()
                .bold()
                .fg_color(Some(Color::Ansi(AnsiColor::Red))),
        )
}

#[derive(clap::Parser)]
#[command(
    about = "An extremely fast C++ runner.",
    version = "1.0.0 (2025-12-25)"
)]
pub(crate) struct Cli {
    #[arg(help = "Path to the source file")]
    pub source: PathBuf,

    #[arg(help = "Path to the input file")]
    pub input: PathBuf,

    #[arg(help = "Path to the output file")]
    pub output: PathBuf,

    #[arg(
        short = 'm',
        long = "max-chars",
        default_value_t = MAX_CHARS,
        help = "Max captured output",
    )]
    pub max_chars: i64,
}
