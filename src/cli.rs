use anstyle::{AnsiColor, Color, Style};
use clap::builder::Styles as ClapStyles;
use std::path::PathBuf;

const MAX_OUTPUT_CHARS: usize = 50_000;

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
#[command(name = "cppu", about = "C++ Utils v1.1.0", version = "1.1.0")]
pub(crate) struct Cli {
    #[arg(help = "Path to the source file")]
    pub source: PathBuf,

    #[arg(
        short = 'i',
        long = "input",
        value_name = "path",
        help = "Read stdin from file (optional; defaults to stdin)"
    )]
    pub input: Option<PathBuf>,

    #[arg(
        short = 'o',
        long = "output",
        value_name = "path",
        help = "Write output to file (optional; defaults to stdout)"
    )]
    pub output: Option<PathBuf>,

    #[arg(
        short = 'a',
        long = "amal",
        value_name = "path",
        help = "Path to output the source file with all includes replace with the actual content of the header files recursively (optional; defaults to false)"
    )]
    pub amal: Option<PathBuf>,

    #[arg(
        short = 'm',
        long = "max-output-chars",
        default_value_t = MAX_OUTPUT_CHARS,
        value_name = "N",
        help = "Max captured output",
    )]
    pub max_output_chars: usize,

    #[arg(short = 'q', long = "quiet", help = "Suppress info logs")]
    pub quiet: bool,

    #[arg(long = "no-clean", help = "Keep compiled .exe")]
    pub no_clean: bool,

    #[arg(long = "use-clang", help = "Use clang++ instead of g++")]
    pub use_clang: bool,

    #[arg(
        long = "cflags",
        default_value = "-O2",
        help = "Extra flags passed to g++"
    )]
    pub cflags: String,
}
