use crate::cli::Cli;
use num_format::{Locale, ToFormattedString};
use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::thread;
use std::time::Duration;

use super::ui::Ui;
use super::{RC_ERROR, absolute_path, file_error, invalid_input, parent_dir, quoted_path};

const POLL_INTERVAL: Duration = Duration::from_millis(25);
const READ_BUFFER_SIZE: usize = 8192;
const OUTPUT_QUEUE_CAPACITY: usize = 16;

pub(super) fn compile_file(
    source: &Path,
    output: &Path,
    cflags: &str,
    use_clang: bool,
    canceled: &AtomicBool,
) -> io::Result<(ExitStatus, Vec<u8>)> {
    let compiler = if use_clang { "clang++" } else { "g++" };
    let flags = shlex::split(cflags).ok_or_else(|| {
        invalid_input("compiler flags contain an unmatched quote or invalid escape")
    })?;
    let mut command = Command::new(compiler);
    command
        .args(flags)
        .arg(absolute_path(source)?)
        .arg("-o")
        .arg(absolute_path(output)?)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    check_canceled(canceled)?;
    let mut process = Process::spawn(&mut command).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("could not start `{compiler}`: {error}"),
        )
    })?;
    let stderr = process
        .0
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("could not capture compiler diagnostics"))?;
    let (sender, receiver) = mpsc::sync_channel(OUTPUT_QUEUE_CAPACITY);
    spawn_reader(stderr, sender);

    let mut diagnostics = Vec::new();
    receive_output(&receiver, canceled, |chunk| {
        diagnostics.extend_from_slice(chunk);
        Ok(())
    })?;
    Ok((process.wait(canceled)?, diagnostics))
}

pub(super) fn execute_and_capture(
    exe: &Path,
    args: &Cli,
    ui: &Ui,
    canceled: &AtomicBool,
) -> io::Result<i32> {
    let exe = absolute_path(exe)?;
    let to_file = args.output.is_some();
    let mut command = Command::new(&exe);
    command
        .current_dir(parent_dir(&exe))
        .stdout(Stdio::piped())
        .stderr(if to_file {
            Stdio::piped()
        } else {
            Stdio::inherit()
        });
    if let Some(path) = &args.input {
        let file = File::open(path).map_err(|error| file_error("open input file", path, error))?;
        command.stdin(file);
    } else {
        command.stdin(Stdio::inherit());
    }

    check_canceled(canceled)?;
    // Delay truncating an existing output file until compilation has succeeded.
    let mut output = Output::new(args.output.as_deref(), ui)?;
    let mut process =
        Process::spawn(&mut command).map_err(|error| file_error("run executable", &exe, error))?;
    let stdout = process
        .0
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("could not capture process output"))?;
    let (sender, receiver) = mpsc::sync_channel(OUTPUT_QUEUE_CAPACITY);
    spawn_reader(stdout, sender.clone());
    if let Some(stderr) = process.0.stderr.take() {
        spawn_reader(stderr, sender.clone());
    }
    drop(sender);

    // Preserve the existing option's byte-based, file-only limit. Raw bytes also
    // preserve binary output; warnings belong on stderr, never inside the file.
    let mut remaining = args.max_output_chars;
    let mut truncated = false;
    loop {
        check_canceled(canceled)?;
        match receiver.recv_timeout(POLL_INTERVAL) {
            Ok(chunk) => {
                let chunk = chunk?;
                let allowed = if to_file {
                    remaining.min(chunk.len())
                } else {
                    chunk.len()
                };
                output.write_all(&chunk[..allowed])?;
                if to_file {
                    remaining -= allowed;
                    if allowed < chunk.len() {
                        truncated = true;
                        break;
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    output.flush()?;
    // Drop the output before diagnostics to separate an unterminated terminal
    // line without ever adding bytes to redirected program output.
    drop(output);

    if truncated {
        ui.warning(format_args!(
            "output exceeded {} bytes; truncated the file and stopped the process",
            args.max_output_chars.to_formatted_string(&Locale::en)
        ));
        return Ok(RC_ERROR);
    }

    let status = process.wait(canceled)?;
    if status.success() {
        return Ok(0);
    }
    ui.error(format_args!(
        "process {} failed ({status})",
        quoted_path(&exe)
    ));
    Ok(status.code().filter(|code| *code != 0).unwrap_or(RC_ERROR))
}

fn spawn_reader(mut reader: impl Read + Send + 'static, sender: SyncSender<io::Result<Vec<u8>>>) {
    thread::spawn(move || {
        let mut buffer = [0; READ_BUFFER_SIZE];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(size) => {
                    if sender.send(Ok(buffer[..size].to_vec())).is_err() {
                        break;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    let _ = sender.send(Err(error));
                    break;
                }
            }
        }
    });
}

fn receive_output(
    receiver: &Receiver<io::Result<Vec<u8>>>,
    canceled: &AtomicBool,
    mut consume: impl FnMut(&[u8]) -> io::Result<()>,
) -> io::Result<()> {
    loop {
        check_canceled(canceled)?;
        match receiver.recv_timeout(POLL_INTERVAL) {
            Ok(chunk) => consume(&chunk?)?,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}

/// Owns the child so early returns cannot leave it running or unreaped.
struct Process(Child);

impl Process {
    fn spawn(command: &mut Command) -> io::Result<Self> {
        command.spawn().map(Self)
    }

    fn wait(&mut self, canceled: &AtomicBool) -> io::Result<ExitStatus> {
        loop {
            check_canceled(canceled)?;
            if let Some(status) = self.0.try_wait()? {
                return Ok(status);
            }
            thread::sleep(POLL_INTERVAL);
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(Some(_))) {
            return;
        }
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/PID", &self.0.id().to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Output {
    writer: Box<dyn Write>,
    separate_status: bool,
    last_byte: Option<u8>,
}

impl Output {
    fn new(path: Option<&Path>, ui: &Ui) -> io::Result<Self> {
        let writer: Box<dyn Write> = match path {
            Some(path) => Box::new(
                File::create(path)
                    .map_err(|error| file_error("create output file", path, error))?,
            ),
            None => Box::new(io::stdout()),
        };
        Ok(Self {
            writer,
            separate_status: path.is_none()
                && !ui.is_quiet()
                && io::stdout().is_terminal()
                && io::stderr().is_terminal(),
            last_byte: None,
        })
    }

    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.writer.write_all(bytes)?;
        if let Some(&byte) = bytes.last() {
            self.last_byte = Some(byte);
        }
        // Keep interactive output responsive, including prompts without newlines.
        self.writer.flush()
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        if self.separate_status && self.last_byte.is_some_and(|byte| byte != b'\n') {
            let _ = writeln!(io::stderr().lock());
        }
    }
}

pub(super) fn check_canceled(canceled: &AtomicBool) -> io::Result<()> {
    if canceled.load(Ordering::Relaxed) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "canceled by user",
        ))
    } else {
        Ok(())
    }
}
