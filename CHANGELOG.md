# Changelog

## [1.1.2] - 2026-03-13

### Fixed

- The program no longer logs that it has created the output file when it actually didn't.

### Changed

- Minor code cleanup in main.rs
- Updated quiet option help message.
- Update dependencies to the latest versions to ensure compatibility and security.

### Removed

## [1.1.1] - 2026-03-12

### Fixed

- Corrected the project's name in the help message and removed the `v` prefix from version number.

### Changed

- Update README and CLI options for clarity and consistency with the new amalgamation feature, and to improve overall readability.

## [1.1.0] - 2026-03-12

### Added

- Amalgamation feature to inline local includes into a single file for easy submission to online judges.
- Detailed usage instructions and options in the README.

### Changed

- Refactor internal code structure for better maintainability and readability, without changing the public API.
- Refactor README to be more concise and focused on usage instructions, moving the "Amalgamation" section into its own dedicated section for clarity.
- Improve formatting of options table for better readability.

### Removed

- Redundant explanations of the project in the README, focusing instead on practical usage and features.
- Outdated examples that didn't reflect the current capabilities of the tool.

## [1.0.0] - 2026-03-09

### Added

- Ability to compile and run C/C++ source files with a single command, without needing to write a Makefile or manually invoke the compiler.
- Ability to pipe custom input into the program's stdin, and capture stdout to a file.
- Option to use `clang++` instead of `g++` as the compiler.
- Option to specify extra flags passed through to the compiler.
- Automatic cleanup of the compiled executable after the run (with an option to disable this).
- Default maximum output capture of 50,000 characters before truncation, with an option to adjust this.
- Informative logging of the compilation and execution process, with an option to suppress logs.
- Cross-platform support for both Unix-like systems and Windows.
- Comprehensive error handling for compilation errors, runtime errors, and file I/O issues.
- Suppress the log when the `--quiet` flag is used, allowing users to focus on the program's output without distraction.
- Detailed README with installation instructions, usage examples, and explanations of all available options.

[1.1.2]: https://github.com/voximir-p/cppu/compare/v1.1.1...v1.1.2
[1.1.1]: https://github.com/voximir-p/cppu/compare/v1.1.0...v1.1.1
[1.1.0]: https://github.com/voximir-p/cppu/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/voximir-p/cppu/releases/tag/v1.0.0
