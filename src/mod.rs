mod amalgamate;
mod process;
mod ui;

use crate::cli::Cli;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use self::amalgamate::amalgamate;
use self::process::{check_canceled, compile_file, execute_and_capture};
use self::ui::{Status, Ui};

const RC_ERROR: i32 = 1;
const RC_INTERRUPTED: i32 = 130;

pub(crate) struct Runner {
    args: Cli,
}

impl Runner {
    pub(crate) fn new(args: Cli) -> Self {
        Self { args }
    }

    pub(crate) fn run(&self) -> i32 {
        let ui = Ui::new(self.args.quiet);
        let canceled = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&canceled);

        if let Err(error) = ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed)) {
            ui.error(format_args!("could not install Ctrl+C handler: {error}"));
            return RC_ERROR;
        }

        match self.try_run(&ui, &canceled) {
            Ok(code) => code,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                ui.warning("canceled by user (Ctrl+C)");
                RC_INTERRUPTED
            }
            Err(error) => {
                ui.error(error);
                RC_ERROR
            }
        }
    }

    fn try_run(&self, ui: &Ui, canceled: &AtomicBool) -> io::Result<i32> {
        let source = &self.args.source;
        let exe = source.with_extension(std::env::consts::EXE_EXTENSION);
        self.validate_paths(&exe)?;
        check_canceled(canceled)?;

        if let Some(destination) = &self.args.amal {
            let content = amalgamate(source, ui, canceled)?;
            check_canceled(canceled)?;
            fs::write(destination, content)
                .map_err(|error| file_error("write amalgamated file", destination, error))?;
        }

        let compile_source = self.args.amal.as_ref().unwrap_or(source);
        ui.status(Status::Compiling, quoted_path(compile_source));
        let compile_started = Instant::now();
        let (status, diagnostics) = compile_file(
            compile_source,
            &exe,
            &self.args.cflags,
            self.args.use_clang,
            canceled,
        )?;
        let compile_elapsed = compile_started.elapsed();
        ui.compiler_output(&diagnostics);

        if !status.success() {
            ui.error(format_args!(
                "could not compile {}",
                quoted_path(compile_source)
            ));
            return Ok(RC_ERROR);
        }
        if !exe.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("compiler did not create executable {}", quoted_path(&exe)),
            ));
        }
        ui.status(
            Status::Finished,
            format_args!("compilation in {:.2}s", compile_elapsed.as_secs_f64()),
        );

        // Always attempt cleanup after execution, including errors and cancellation.
        let result = check_canceled(canceled).and_then(|()| {
            ui.status(Status::Running, quoted_path(&exe));
            execute_and_capture(&exe, &self.args, ui, canceled)
        });
        let cleanup = if self.args.no_clean {
            Ok(())
        } else {
            clean_exe(&exe, ui)
        };

        // A cleanup failure must not hide the original execution failure.
        let code = match result {
            Ok(code) => code,
            Err(error) => {
                if let Err(cleanup_error) = cleanup {
                    ui.error(cleanup_error);
                }
                return Err(error);
            }
        };
        if let Err(error) = cleanup {
            ui.error(error);
            return Ok(if code == 0 { RC_ERROR } else { code });
        }

        if code == 0 {
            match &self.args.output {
                Some(path) => ui.status(
                    Status::Finished,
                    format_args!("output written to {}", quoted_path(path)),
                ),
                None => ui.status(Status::Finished, "output written to stdout"),
            }
        }
        Ok(code)
    }

    fn validate_paths(&self, exe: &Path) -> io::Result<()> {
        for (label, path) in [
            ("source", Some(self.args.source.as_path())),
            ("input", self.args.input.as_deref()),
        ] {
            if let Some(path) = path {
                let metadata = fs::metadata(path)
                    .map_err(|error| file_error(&format!("read {label} file"), path, error))?;
                if !metadata.is_file() {
                    return Err(invalid_input(format!(
                        "{label} is not a file: {}",
                        quoted_path(path)
                    )));
                }
            }
        }

        let paths = [
            ("source", Some(self.args.source.as_path()), false),
            ("input", self.args.input.as_deref(), false),
            ("executable", Some(exe), true),
            ("output", self.args.output.as_deref(), true),
            ("amalgamated source", self.args.amal.as_deref(), true),
        ];
        for (index, &(label, path, writable)) in paths.iter().enumerate() {
            let Some(path) = path else { continue };
            if writable {
                let parent = parent_dir(path);
                if !parent.is_dir() {
                    return Err(invalid_input(format!(
                        "{label} directory does not exist: {}",
                        quoted_path(parent)
                    )));
                }
            }
            for &(other_label, other_path, other_writable) in &paths[..index] {
                if let Some(other) = other_path
                    && (writable || other_writable)
                    && same_file(path, other)?
                {
                    return Err(invalid_input(format!(
                        "{label} and {other_label} must use different files: {}",
                        quoted_path(path)
                    )));
                }
            }
        }
        Ok(())
    }
}

fn clean_exe(exe: &Path, ui: &Ui) -> io::Result<()> {
    match fs::remove_file(exe) {
        Ok(()) => {
            ui.status(Status::Removed, quoted_path(exe));
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(file_error("remove executable", exe, error)),
    }
}

fn invalid_input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn file_error(action: &str, path: &Path, error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("could not {action} {}: {error}", quoted_path(path)),
    )
}

fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn absolute_path(path: &Path) -> io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn quoted_path(path: &Path) -> String {
    let display = absolute_path(path).unwrap_or_else(|_| path.to_path_buf());
    let display = std::env::current_dir()
        .ok()
        .and_then(|cwd| display.strip_prefix(cwd).ok().map(Path::to_path_buf))
        .filter(|relative| !relative.as_os_str().is_empty())
        .unwrap_or(display);
    format!("`{}`", display.display())
}

fn same_file(left: &Path, right: &Path) -> io::Result<bool> {
    fn identity(path: &Path) -> io::Result<PathBuf> {
        match fs::canonicalize(path) {
            Ok(path) => Ok(path),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let name = path
                    .file_name()
                    .ok_or_else(|| invalid_input("expected a file path"))?;
                Ok(fs::canonicalize(parent_dir(path))?.join(name))
            }
            Err(error) => Err(error),
        }
    }
    if identity(left)? == identity(right)? {
        return Ok(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(left), Ok(right)) = (fs::metadata(left), fs::metadata(right)) {
            return Ok(left.dev() == right.dev() && left.ino() == right.ino());
        }
    }
    Ok(false)
}
