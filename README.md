# cppu

**cppu (C/C++ Utility)** is a C/C++ utility tool for competitive programming. It can compile your code, read input from a file, write output to another file, and amalgamate local `#include`s into a single self-contained source.

---

## Features

- Compiles a `.c`/`.cpp` file with `g++` (or `clang++`) and runs it in one command
- **Amalgamation** — recursively inlines `#include "…"` directives into a single file, perfect for submitting to online judges
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
| `-a, --amal <path>` | off | Amalgamate all local `#include "…"` into a single file and compile that instead (see [Amalgamation](#amalgamation)) |
| `-m, --max-output-chars <N>` | `50000` | Maximum captured characters before output is truncated |
| `-q, --quiet` | off | Suppress all `[INFO]` / `[SUCCESS]` / `[WARNING]` log lines |
| `--no-clean` | off | Keep the compiled `.exe` after the run |
| `--use-clang` | off | Use `clang++` instead of `g++` |
| `--cflags <flags>` | `"-O2"` | Extra flags forwarded to the compiler |

---

## Amalgamation

When you pass `-a <path>`, cppu will produce a **single self-contained source file** before compilation:

1. Every `#include "header.h"` directive (quoted, local includes) is replaced with the full contents of the referenced file, resolved relative to the current working directory.
2. This replacement is applied **recursively**, so nested local includes are inlined too.
3. System/angle-bracket includes (`#include <vector>`, etc.) are left untouched.
4. Circular includes are detected and cause an immediate error.
5. The amalgamated result is written to `<path>`, and **that file** is what gets compiled and executed instead of the original source.

This is especially useful for competitive programming, where you maintain a personal header library but need to submit a single `.c`/`.cpp` file to an online judge.

### Example

Given the following project layout:

```
solution.cpp      ← #include "dsu.h"
dsu.h             ← your disjoint-set-union header
```

```sh
# Produce an amalgamated file and run it
cppu solution.cpp -a merged.cpp -i test.in -o test.out
```

After this command, `merged.cpp` will contain the full source with the contents of `dsu.h` inlined in place of `#include "dsu.h"`. You can submit `merged.cpp` directly.

---

## Examples

```sh
# Compile and run hello.cpp (output printed to terminal)
cppu hello.cpp

# Provide a test input file; capture output
cppu solution.cpp -i tests/01.in -o tests/01.out

# Amalgamate local includes, compile, and run
cppu solution.cpp -a merged.cpp

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

1. If `-a <path>` is given, recursively inlines all local `#include "…"` directives and writes the amalgamated source to `<path>`.
2. Compiles the source (or the amalgamated file, if produced) to a temporary `.exe` next to the source file using `g++` (or `clang++`).
3. Runs the resulting binary, wiring stdin/stdout/stderr as configured.
4. Waits for the process to finish (or for Ctrl+C).
5. Deletes the temporary binary unless `--no-clean` was passed.
6. Exits with the same code the child process returned.

All diagnostic messages (compile errors, timing, status) are printed to **stderr** so they never pollute captured output.

---

## License

cppu is licensed under the GNU General Public License v3.0 ([LICENSE](LICENSE) or <https://www.gnu.org/licenses/gpl-3.0.html#license-text>)
