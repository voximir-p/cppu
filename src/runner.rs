use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use chrono::Local;

use crate::cli::Cli;

const RC_TRUNCATED_OUTPUT: i32 = 1;

const SUCCESS: &str = "SUCCESS";
const INFO: &str = "INFO";
const ERROR_: &str = "ERROR";
const WARNING: &str = "WARNING";

const CLR_RESET: &str = "\x1b[0m";
const SUCCESS_LABEL: &str = "\x1b[1m\x1b[38;2;119;239;119m";
const INFO_LABEL: &str = "\x1b[1m\x1b[38;2;97;190;255m";
const ERROR_LABEL: &str = "\x1b[1;91m";
const WARNING_LABEL: &str = "\x1b[1m\x1b[38;2;255;215;95m";

const INFO_BODY: &str = "\x1b[2m";
const ERROR_BODY: &str = "\x1b[38;2;255;95;95m";

pub(crate) struct Runner {
    args: Cli,
}

impl Runner {
    pub(crate) fn new(args: Cli) -> Self {
        Self { args }
    }

    pub(crate) fn run(&self) -> i32 {
        let show_status_logs = !self.args.quiet;
        let cancel_requested = Arc::new(AtomicBool::new(false));
        let active_child = Arc::new(Mutex::new(None::<u32>));

        {
            let cancel_requested = Arc::clone(&cancel_requested);
            let active_child = Arc::clone(&active_child);
            let _ = ctrlc::set_handler(move || {
                cancel_requested.store(true, Ordering::SeqCst);
                if let Ok(guard) = active_child.lock()
                    && let Some(pid) = *guard
                {
                    let _ = Command::new("taskkill")
                        .args(["/PID", &pid.to_string(), "/T", "/F"])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                }
            });
        }

        let source = self.args.source.clone();
        let mut exe = source.clone();
        exe.set_extension("exe");

        if let Some(input) = &self.args.input
            && !input.exists()
        {
            log_line(
                ERROR_,
                format!("Input file not found: {}", abs_string(input)),
            );
            return 1;
        }

        if let Some(output) = &self.args.output {
            if let Some(parent) = output.parent()
                && !parent.as_os_str().is_empty()
                && !parent.exists()
            {
                log_line(
                    ERROR_,
                    format!("Output directory not found: {}", abs_string(parent)),
                );
                return 1;
            }

            if let Err(err) = File::create(output) {
                log_line(
                    ERROR_,
                    format!("Cannot create output file: {}", abs_string(output)),
                );
                if !self.args.quiet {
                    log_line(ERROR_, err.to_string());
                }
                return 1;
            }

            if show_status_logs {
                log_line(INFO, format!("Created output file: {}", abs_string(output)));
            }
        }

        if cancel_requested.load(Ordering::SeqCst) {
            if !self.args.quiet {
                log_line(WARNING, "Canceled by user (Ctrl+C)");
            }
            return 1;
        }

        if let Some(amal) = &self.args.amal {
            let mut seen = HashSet::new();
            if let Some(content) = self.amalgamate(show_status_logs, &source, &mut seen) {
                match File::create(amal) {
                    Ok(mut file) => {
                        if let Err(err) = file.write_all(content.as_bytes()) {
                            log_line(
                                ERROR_,
                                format!("Failed to write amalgamated file: {}", abs_string(amal)),
                            );
                            log_line(ERROR_, err.to_string());
                        }
                    }
                    Err(err) => {
                        log_line(
                            ERROR_,
                            format!("Cannot create amalgamated file: {}", abs_string(amal)),
                        );
                        log_line(ERROR_, err.to_string());
                    }
                }
            }
        }

        let compile_source = self.args.amal.as_ref().unwrap_or(&source);

        if show_status_logs {
            log_line(
                INFO,
                format!("Compiling file: {}", abs_string(compile_source)),
            );
        }

        let (compile_status, compile_stderr) =
            match compile_file(compile_source, &exe, &self.args.cflags, self.args.use_clang) {
                Ok(v) => v,
                Err(err) => {
                    log_line(ERROR_, "Failed to start compiler");
                    if !self.args.quiet {
                        log_line(ERROR_, err.to_string());
                    }
                    return 1;
                }
            };

        if compile_status.success() && exe.exists() {
            if show_status_logs {
                log_line(
                    INFO,
                    format!("Testing compiled executable: {}", abs_string(&exe)),
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

            if show_status_logs {
                if let Some(output) = &self.args.output {
                    log_line(
                        SUCCESS,
                        format!("Output written to: {}", abs_string(output)),
                    );
                } else {
                    println!();
                    println!();
                    log_line(SUCCESS, "Output written to stdout");
                }
            }
        } else if compile_status.success() {
            if !compile_stderr.trim().is_empty() {
                log_line(ERROR_, "Compiler output:");
                eprint!("{}", compile_stderr);
            }
            log_line(
                ERROR_,
                format!(
                    "Compilation successful but cannot find output file: {}",
                    abs_string(&exe)
                ),
            );
            return 1;
        } else {
            if !compile_stderr.trim().is_empty() {
                log_line(ERROR_, "Compiler output:");
                eprint!("{}", compile_stderr);
                eprintln!();
            }
            log_line(ERROR_, "Compilation failed!");
            return 1;
        }

        if !self.args.no_clean {
            let rc = clean_exe(&exe, show_status_logs);
            if rc != 0 {
                return rc;
            }
        }

        if show_status_logs {
            log_inline(INFO, "Exiting...");
            let _ = io::stdout().flush();
        }

        0
    }

    fn amalgamate(
        &self,
        show_status_logs: bool,
        source: &Path,
        seen: &mut HashSet<PathBuf>,
    ) -> Option<String> {
        let canonical = fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
        if !seen.insert(canonical.clone()) {
            log_line(
                ERROR_,
                format!("Circular include detected: {}", abs_string(source)),
            );
            std::process::exit(1);
        }

        let source_file = File::open(source).unwrap();
        let reader = BufReader::new(source_file);
        let mut content = String::new();

        for line in reader.lines().map_while(Result::ok) {
            let line_t = line.trim();
            if !line_t.starts_with("#include") || !line_t.contains('"') {
                content.push_str(&line);
                content.push('\n');
                continue;
            }

            let parts: Vec<&str> = line_t.split('"').collect();
            if parts.len() < 2 {
                if !self.args.quiet {
                    log_line(ERROR_, format!("Malformed include directive: {}", line_t));
                }
                std::process::exit(1);
            }

            let included_file = parts[1];
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let included_path = cwd.join(included_file);
            if !included_path.exists() || !included_path.is_file() {
                log_line(
                    ERROR_,
                    format!("Included file not found: {}", abs_string(&included_path)),
                );
                std::process::exit(1);
            }
            if show_status_logs {
                log_line(
                    INFO,
                    format!("Amalgamating with: {}", abs_string(&included_path)),
                );
            }

            let included_content = self.amalgamate(show_status_logs, &included_path, seen)?;
            content.push_str(&included_content);
            content.push('\n');
        }

        Some(content)
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
    let to_file = output_path.is_some();
    let exe_path = match absolute_path(exe) {
        Ok(path) => path,
        Err(err) => {
            if !quiet {
                log_line(
                    ERROR_,
                    format!("Failed to resolve executable path: {}", err),
                );
            }
            return 1;
        }
    };

    let mut command = Command::new(&exe_path);
    command.stdout(Stdio::piped());
    if to_file {
        command.stderr(Stdio::piped());
    } else {
        command.stderr(Stdio::inherit());
    }
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
                    log_line(
                        ERROR_,
                        format!("Failed to open input file: {}", abs_string(inp)),
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
                log_line(ERROR_, format!("Failed to start process: {}", err));
                log_line(ERROR_, format!("Executable path: {}", exe_path.display()));
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
                log_line(ERROR_, "Failed to capture process output");
            }
            return 1;
        }
    };

    let stderr = if to_file {
        match child.stderr.take() {
            Some(s) => Some(s),
            None => {
                let _ = child.kill();
                let _ = child.wait();
                if let Ok(mut guard) = active_child.lock() {
                    *guard = None;
                }
                if !quiet {
                    log_line(ERROR_, "Failed to capture process error output");
                }
                return 1;
            }
        }
    } else {
        None
    };

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let stdout_tx = tx.clone();
    let stdout_reader_handle = thread::spawn(move || {
        let mut reader = stdout;
        let mut buf = vec![0_u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if stdout_tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    let stderr_reader_handle = stderr.map(|stderr_pipe| {
        thread::spawn(move || {
            let mut reader = stderr_pipe;
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
        })
    });
    let mut out_file = match output_path {
        Some(path) => match File::create(path) {
            Ok(file) => Some(file),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader_handle.join();
                if let Some(handle) = stderr_reader_handle {
                    let _ = handle.join();
                }
                if let Ok(mut guard) = active_child.lock() {
                    *guard = None;
                }
                if !quiet {
                    log_line(
                        ERROR_,
                        format!("Cannot open output file for writing: {}", abs_string(path)),
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
                log_line(WARNING, "Execution interrupted (Ctrl+C)");
            }
            terminate_and_wait(&mut child, 2000, quiet);
            let _ = stdout_reader_handle.join();
            if let Some(handle) = stderr_reader_handle {
                let _ = handle.join();
            }
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
                            log_line(
                                WARNING,
                                format!(
                                    "Output exceeds {} characters! Truncating output.",
                                    max_chars
                                ),
                            );
                        }

                        terminate_and_wait(&mut child, 2000, quiet);

                        if let Some(file) = out_file.as_mut() {
                            let _ = writeln!(file, "Output exceeds {} characters!\n", max_chars);
                            let _ = file.write_all(&buffered);
                            let _ = file.flush();
                        }

                        let _ = stdout_reader_handle.join();
                        if let Some(handle) = stderr_reader_handle {
                            let _ = handle.join();
                        }
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
    let _ = stdout_reader_handle.join();
    if let Some(handle) = stderr_reader_handle {
        let _ = handle.join();
    }

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
        log_line(
            WARNING,
            "Abortion took too long! Killing process immediately.",
        );
    }

    let _ = child.kill();
    let _ = child.wait();
}

fn clean_exe(output: &Path, show_status_logs: bool) -> i32 {
    if output.exists() {
        match fs::remove_file(output) {
            Ok(_) => {
                if show_status_logs {
                    log_line(
                        SUCCESS,
                        format!("Deleted output file: {}", abs_string(output)),
                    );
                }
                0
            }
            Err(err) => {
                log_line(ERROR_, "Error:");
                log_line(ERROR_, err.to_string());
                log_line(
                    ERROR_,
                    format!("Cannot delete output file: {}", abs_string(output)),
                );
                1
            }
        }
    } else {
        log_line(
            WARNING,
            format!(
                "Output file not found! Skipping deletion: {}",
                abs_string(output)
            ),
        );
        1
    }
}

fn timestamp() -> String {
    Local::now().format("%H:%M:%S").to_string()
}

fn log_line(label: &str, message: impl AsRef<str>) {
    let (label_style, body_style) = log_styles(label);
    eprintln!(
        "[{}] {}{}{} {}{}{}",
        timestamp(),
        label_style,
        label,
        CLR_RESET,
        body_style,
        message.as_ref(),
        CLR_RESET
    );
}

fn log_inline(label: &str, message: impl AsRef<str>) {
    let (label_style, body_style) = log_styles(label);
    eprint!(
        "[{}] {}{}{} {}{}{}",
        timestamp(),
        label_style,
        label,
        CLR_RESET,
        body_style,
        message.as_ref(),
        CLR_RESET
    );
}

fn log_styles(label: &str) -> (&'static str, &'static str) {
    match label {
        SUCCESS => (SUCCESS_LABEL, ""),
        INFO => (INFO_LABEL, INFO_BODY),
        ERROR_ => (ERROR_LABEL, ERROR_BODY),
        WARNING => (WARNING_LABEL, ""),
        _ => ("", ""),
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

    normalize_for_display(&base).display().to_string()
}

fn normalize_for_display(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(comp.as_os_str());
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn absolute_path(path: &Path) -> io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}
