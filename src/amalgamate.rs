use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use super::process::check_canceled;
use super::ui::{Status, Ui};
use super::{file_error, invalid_input, parent_dir, quoted_path};

pub(super) fn amalgamate(source: &Path, ui: &Ui, canceled: &AtomicBool) -> io::Result<String> {
    expand_includes(source, &mut HashSet::new(), ui, canceled)
}

fn expand_includes(
    source: &Path,
    active_includes: &mut HashSet<PathBuf>,
    ui: &Ui,
    canceled: &AtomicBool,
) -> io::Result<String> {
    check_canceled(canceled)?;
    let canonical = fs::canonicalize(source)
        .map_err(|error| file_error("resolve source file", source, error))?;
    if !active_includes.insert(canonical.clone()) {
        return Err(invalid_input(format!(
            "circular include detected: {}",
            quoted_path(source)
        )));
    }

    // Track only the current include chain; shared headers are not cycles.
    let result = (|| {
        let file =
            File::open(source).map_err(|error| file_error("open source file", source, error))?;
        let mut content = String::new();

        for (index, line) in BufReader::new(file).lines().enumerate() {
            check_canceled(canceled)?;
            let line = line.map_err(|error| file_error("read source file", source, error))?;
            match local_include(&line).map_err(|error| {
                invalid_input(format!("{}:{}: {error}", quoted_path(source), index + 1))
            })? {
                Some(include) => {
                    let path = parent_dir(source).join(include);
                    ui.status(Status::Amalgamating, quoted_path(&path));
                    content.push_str(&expand_includes(&path, active_includes, ui, canceled)?);
                }
                None => {
                    content.push_str(&line);
                    content.push('\n');
                }
            }
        }
        Ok(content)
    })();

    active_includes.remove(&canonical);
    result
}

// This is a textual expander for quoted includes, not a C/C++ preprocessor.
fn local_include(line: &str) -> io::Result<Option<&str>> {
    let Some(directive) = line.trim_start().strip_prefix('#') else {
        return Ok(None);
    };
    let Some(rest) = directive.trim_start().strip_prefix("include") else {
        return Ok(None);
    };
    if rest.starts_with(|ch: char| ch.is_ascii_alphanumeric() || ch == '_') {
        return Ok(None);
    }
    let Some(quoted) = rest.trim_start().strip_prefix('"') else {
        return Ok(None);
    };
    let Some((path, _)) = quoted.split_once('"') else {
        return Err(invalid_input("unterminated quoted include"));
    };
    if path.is_empty() {
        return Err(invalid_input("empty quoted include"));
    }
    Ok(Some(path))
}
