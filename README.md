# cppu

**cppu** is a C++ utility tool for competitive programming. It can compile your code, use an input from a file and can output to another file.

---

## Features

- Compiles a `.cpp` file with `g++` (or `clang++`) and runs it in one command
- Pipes stdin from a file with `-i` and captures stdout+stderr to a file with `-o`
- Timestamped, color-coded log output on stderr
- Output truncation guard (`-m`) to prevent runaway programs from flooding files
- Ctrl+C cleanly kills the child process
- Auto-deletes the compiled binary after execution (use `--no-clean` to keep it)

---

## Prerequisites

| Requirement | Notes |
| --- | --- |
| **Rust + Cargo** | Install from [rustup.rs](https://rustup.rs) |
| **g++** | MinGW-w64 on Windows; `g++` must be on `PATH` |
| **clang++** | Optional; only needed if you use `--use-clang` |

On Windows, the easiest way to get `g++` is via [MSYS2](https://www.msys2.org/) (`pacman -S mingw-w64-ucrt-x86_64-gcc`) and adding its `bin/` directory to your `PATH`.

---

## Building

```sh
git clone https://github.com/voximir-p/cppu
cd cppu
cargo build --release
```

The binary will be at `target/release/cppu.exe`. Copy it anywhere on your `PATH`.

```sh
# Quick install to ~/.cargo/bin (already on PATH if you used rustup)
cargo install --path .
```

---

## Usage

```sh
cppu [OPTIONS] <source>
```

Run `cppu` with no arguments to see the full help screen.

### Options

| Flag | Default | Description |
| --- | --- | --- |
| `<source>` | *(required)* | Path to the `.cpp` source file |
| `-i, --input <path>` | stdin | Feed a file as the program's stdin |
| `-o, --output <path>` | stdout | Capture program output (stdout + stderr) to a file |
| `-m, --max-output-chars <N>` | `50000` | Maximum captured characters before output is truncated |
| `-q, --quiet` | off | Suppress all `[INFO]` / `[SUCCESS]` / `[WARNING]` log lines |
| `--no-clean` | off | Keep the compiled `.exe` after the run |
| `--use-clang` | off | Use `clang++` instead of `g++` |
| `--cflags <flags>` | `"-O2"` | Extra flags forwarded to the compiler |

---

## Examples

```sh
# Compile and run hello.cpp (output printed to terminal)
cppu hello.cpp

# Provide a test input file; capture output
cppu solution.cpp -i tests/01.in -o tests/01.out

# Use C++20 with extra warnings
cppu main.cpp --cflags "-std=c++20 -Wall -Wextra"

# Use clang++ and keep the compiled binary
cppu main.cpp --use-clang --no-clean

# Pipe your own stdin interactively (no -i flag needed)
echo "42" | cppu main.cpp
```

---

## Exit Codes

| Code | Meaning |
| --- | --- |
| `0` | Program exited successfully |
| `1` | Output was truncated (program hit the `-m` limit) |
| *(program's own code)* | Whatever exit code the child process returned |
| Non-zero | Compilation failed, or the process could not be launched |

---

## How It Works

1. Compiles `<source>` to a temporary `.exe` next to the source file using `g++` (or `clang++`).
2. Runs the resulting binary, wiring stdin/stdout/stderr as configured.
3. Waits for the process to finish (or for Ctrl+C).
4. Deletes the temporary binary unless `--no-clean` was passed.
5. Exits with the same code the child process returned.

All diagnostic messages (compile errors, timing, status) are printed to **stderr** so they never pollute captured output.
