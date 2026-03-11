use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::cli::Cli;

const RC_TRUNCATED_OUTPUT: i32 = 1;

const CLR_RESET: &str = "\x1b[0m";
const SUCCESS: &str = "\x1b[1m\x1b[48;2;119;239;119m\x1b[38;2;54;53;55m SUCCESS \x1b[0m\x1b[38;2;119;239;119m";
const INFO: &str = "\x1b[1m\x1b[48;2;97;190;255m\x1b[38;2;54;53;55m INFO \x1b[0m";
const ERROR_: &str = "\x1b[1m\x1b[48;2;255;95;95m\x1b[38;2;54;53;55m ERROR \x1b[0m\x1b[38;2;255;95;95m";
const WARNING: &str = "\x1b[1m\x1b[48;2;255;215;95m\x1b[38;2;54;53;55m WARNING \x1b[0m\x1b[38;2;255;215;95m";

pub(crate) struct Runner {
    args: Cli,
}

impl Runner {
    pub(crate) fn new(args: Cli) -> Self {
        Self { args }
    }

    pub(crate) fn run(&self) -> i32 {
        let cancel_requested = Arc::new(AtomicBool::new(false));
        let active_child = Arc::new(Mutex::new(None::<u32>));

        {
            let cancel_requested = Arc::clone(&cancel_requested);
            let active_child = Arc::clone(&active_child);
            let _ = ctrlc::set_handler(move || {
                cancel_requested.store(true, Ordering::SeqCst);
                if let Ok(guard) = active_child.lock() {
                    if let Some(pid) = *guard {
                        let _ = Command::new("taskkill")
                            .args(["/PID", &pid.to_string(), "/T", "/F"])
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .status();
                    }
                }
            });
        }

        let source = self.args.source.clone();
        let mut exe = source.clone();
        exe.set_extension("exe");

        if let Some(input) = &self.args.input {
            if !input.exists() {
                println!(
                    "{} Input file not found: {}{}",
                    ERROR_,
                    abs_string(input),
                    CLR_RESET
                );
                return 1;
            }
        }

        if let Some(output) = &self.args.output {
            if let Some(parent) = output.parent()
                && !parent.as_os_str().is_empty() && !parent.exists()
            {
                println!(
                    "{} Output directory not found: {}{}",
                    ERROR_,
                    abs_string(parent),
                    CLR_RESET
                );
                return 1;
            }

            if let Err(err) = File::create(output) {
                println!(
                    "{} Cannot create output file: {}{}",
                    ERROR_,
                    abs_string(output),
                    CLR_RESET
                );
                if !self.args.quiet {
                    println!("{}{}", err, CLR_RESET);
                }
                return 1;
            }

            if !self.args.quiet {
                println!(
                    "{} Created output file: {}{}",
                    INFO,
                    abs_string(output),
                    CLR_RESET
                );
            }
        }

        if cancel_requested.load(Ordering::SeqCst) {
            if !self.args.quiet {
                println!("{} Canceled by user (Ctrl+C){}", WARNING, CLR_RESET);
            }
            return 1;
        }

        if !self.args.quiet {
            println!(
                "{} Compiling file: {}{}",
                INFO,
                abs_string(&source),
                CLR_RESET
            );
        }

        let (compile_status, compile_stderr) = match compile_file(
            &source,
            &exe,
            &self.args.cflags,
            self.args.use_clang,
        ) {
            Ok(v) => v,
            Err(err) => {
                println!("{} Failed to start compiler{}", ERROR_, CLR_RESET);
                if !self.args.quiet {
                    println!("{}{}", err, CLR_RESET);
                }
                return 1;
            }
        };

        if compile_status.success() && exe.exists() {
            if !self.args.quiet {
                println!(
                    "{} Testing output file: {}{}",
                    INFO,
                    abs_string(&exe),
                    CLR_RESET
                );
            }

            let exec_rc = execute_and_capture(
                &exe,
                self.args.input.as_deref(),
                self.args.output.as_deref(),
                self.args.max_output_chars,
                self.args.quiet,
                Arc::clone(&cancel_requested),
                Arc::clone(&active_child),
            );

            if exec_rc != 0 {
                if !self.args.no_clean && exe.exists() {
                    let _ = clean_exe(&exe, self.args.quiet);
                }
                return exec_rc;
            }

            if !self.args.quiet {
                if let Some(output) = &self.args.output {
                    println!(
                        "{} Output written to: {}{}",
                        SUCCESS,
                        abs_string(output),
                        CLR_RESET
                    );
                } else {
                    println!("\n\n{} Output written to stdout{}", SUCCESS, CLR_RESET);
                }
            }
        } else {
            if compile_status.success() && !exe.exists() {
                print!("{}{}", compile_stderr, CLR_RESET);
                println!(
                    "{} Compilation successful but cannot find output file: {}{}",
                    ERROR_,
                    abs_string(&exe),
                    CLR_RESET
                );
            } else {
                println!("{} Compiler output:{}", ERROR_, CLR_RESET);
                print!("{}{}", compile_stderr, CLR_RESET);
                println!();
                println!("{} Compilation failed!{}", ERROR_, CLR_RESET);
            }
            return 1;
        }

        if !self.args.no_clean {
            let rc = clean_exe(&exe, self.args.quiet);
            if rc != 0 {
                return rc;
            }
        }

        if !(self.args.quiet) {
            print!("{} Exiting...{}", INFO, CLR_RESET);
            let _ = io::stdout().flush();
        }

        0
    }
}

fn compile_file(
    source: &Path,
    output: &Path,
    cflags: &str,
    use_clang: bool,
) -> io::Result<(ExitStatus, String)> {
    let compiler = if use_clang { "clang++" } else { "g++" };
    let mut cmd = Command::new(compiler);

    let mut parsed_flags = shlex::split(cflags).unwrap_or_else(|| vec![cflags.to_string()]);
    if parsed_flags.len() == 1 && parsed_flags[0].is_empty() {
        parsed_flags.clear();
    }

    let source_path = absolute_path(source)?;
    let output_path = absolute_path(output)?;

    cmd.args(parsed_flags)
        .arg(&source_path)
        .arg("-o")
        .arg(&output_path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let output = cmd.output()?;
    Ok((
        output.status,
        String::from_utf8_lossy(&output.stderr).into_owned(),
    ))
}

fn execute_and_capture(
    exe: &Path,
    input: Option<&Path>,
    output_path: Option<&Path>,
    max_chars: usize,
    quiet: bool,
    cancel_requested: Arc<AtomicBool>,
    active_child: Arc<Mutex<Option<u32>>>,
) -> i32 {
    let exe_path = match absolute_path(exe) {
        Ok(path) => path,
        Err(err) => {
            if !quiet {
                println!(
                    "{} Failed to resolve executable path: {}{}",
                    ERROR_, err, CLR_RESET
                );
            }
            return 1;
        }
    };

    let mut command = Command::new(&exe_path);
    command.stdout(Stdio::piped()).stderr(Stdio::inherit());
    if let Some(parent) = exe_path.parent() {
        command.current_dir(parent);
    }

    if let Some(inp) = input {
        match File::open(inp) {
            Ok(file) => {
                command.stdin(Stdio::from(file));
            }
            Err(_) => {
                if !quiet {
                    println!(
                        "{} Failed to open input file: {}{}",
                        ERROR_,
                        abs_string(inp),
                        CLR_RESET
                    );
                }
                return 1;
            }
        }
    } else {
        command.stdin(Stdio::inherit());
    }

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(err) => {
            if !quiet {
                println!(
                    "{} Failed to start process: {}{}",
                    ERROR_, err, CLR_RESET
                );
                println!(
                    "{} Executable path: {}{}",
                    ERROR_,
                    exe_path.display(),
                    CLR_RESET
                );
            }
            return 1;
        }
    };

    if let Ok(mut guard) = active_child.lock() {
        *guard = Some(child.id());
    }

    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            if let Ok(mut guard) = active_child.lock() {
                *guard = None;
            }
            if !quiet {
                println!("{} Failed to capture process output{}", ERROR_, CLR_RESET);
            }
            return 1;
        }
    };

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let reader_handle = thread::spawn(move || {
        let mut reader = stdout;
        let mut buf = vec![0_u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let to_file = output_path.is_some();
    let mut out_file = match output_path {
        Some(path) => match File::create(path) {
            Ok(file) => Some(file),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader_handle.join();
                if let Ok(mut guard) = active_child.lock() {
                    *guard = None;
                }
                if !quiet {
                    println!(
                        "{} Cannot open output file for writing: {}{}",
                        ERROR_,
                        abs_string(path),
                        CLR_RESET
                    );
                }
                return 1;
            }
        },
        None => None,
    };

    let mut buffered: Vec<u8> = Vec::with_capacity(max_chars.min(1 << 20));
    let mut total = 0_usize;
    let mut finished = false;

    while !finished {
        if cancel_requested.load(Ordering::SeqCst) {
            if !quiet {
                println!("{} Execution interrupted (Ctrl+C){}", WARNING, CLR_RESET);
            }
            terminate_and_wait(&mut child, 2000, quiet);
            let _ = reader_handle.join();
            if let Ok(mut guard) = active_child.lock() {
                *guard = None;
            }
            return 1;
        }

        loop {
            match rx.try_recv() {
                Ok(chunk) => {
                    if !to_file {
                        let _ = io::stdout().write_all(&chunk);
                        let _ = io::stdout().flush();
                        continue;
                    }

                    if total + chunk.len() > max_chars {
                        let allowed = max_chars.saturating_sub(total);
                        if allowed > 0 {
                            buffered.extend_from_slice(&chunk[..allowed]);
                        }

                        if !quiet {
                            println!(
                                "{} Output exceeds {} characters! Truncating output.{}",
                                WARNING, max_chars, CLR_RESET
                            );
                        }

                        terminate_and_wait(&mut child, 2000, quiet);

                        if let Some(file) = out_file.as_mut() {
                            let _ = writeln!(file, "Output exceeds {} characters!\n", max_chars);
                            let _ = file.write_all(&buffered);
                            let _ = file.flush();
                        }

                        let _ = reader_handle.join();
                        if let Ok(mut guard) = active_child.lock() {
                            *guard = None;
                        }
                        return RC_TRUNCATED_OUTPUT;
                    }

                    buffered.extend_from_slice(&chunk);
                    total += chunk.len();
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    finished = true;
                    break;
                }
            }
        }

        if !finished {
            if let Ok(Some(_status)) = child.try_wait() {
                // Let reader thread flush remaining bytes and close channel.
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    let _ = child.wait();
    let _ = reader_handle.join();

    if let Some(file) = out_file.as_mut() {
        let _ = file.write_all(&buffered);
        let _ = file.flush();
    } else {
        let _ = io::stdout().flush();
    }

    if let Ok(mut guard) = active_child.lock() {
        *guard = None;
    }

    0
}

fn terminate_and_wait(child: &mut Child, timeout_ms: u64, quiet: bool) {
    let _ = child.kill();
    for _ in 0..(timeout_ms / 10) {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }

    if !quiet {
        println!(
            "{} Abortion took too long! Killing process immediately.{}",
            WARNING, CLR_RESET
        );
    }

    let _ = child.kill();
    let _ = child.wait();
}

fn clean_exe(output: &Path, quiet: bool) -> i32 {
    if output.exists() {
        match fs::remove_file(output) {
            Ok(_) => {
                if !quiet {
                    println!(
                        "{} Deleted output file: {}{}",
                        SUCCESS,
                        abs_string(output),
                        CLR_RESET
                    );
                }
                0
            }
            Err(err) => {
                if !quiet {
                    println!("{} Error:{}", ERROR_, CLR_RESET);
                    println!("{}", err);
                    println!(
                        "{} Cannot delete output file: {}{}",
                        ERROR_,
                        abs_string(output),
                        CLR_RESET
                    );
                }
                1
            }
        }
    } else {
        if !quiet {
            println!(
                "{} Output file not found! Skipping deletion: {}{}",
                WARNING,
                abs_string(output),
                CLR_RESET
            );
        }
        1
    }
}

fn abs_string(path: &Path) -> String {
    let base = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };

    base.display().to_string()
}

fn absolute_path(path: &Path) -> io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}
