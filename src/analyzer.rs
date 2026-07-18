use crate::config::Config;
use crate::types::{Diagnostic, Rule, Summary};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Analysis {
    pub diagnostics: Vec<Diagnostic>,
    pub summary: Summary,
}

pub fn analyze_paths(paths: &[PathBuf], config: &Config) -> Result<Analysis, String> {
    let files = collect_rust_files(paths, config)?;
    let mut diagnostics = Vec::new();
    for path in &files {
        let source = fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        diagnostics.extend(analyze_source(path, &source, config));
    }
    diagnostics.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.line.cmp(&right.line))
            .then(left.rule.code().cmp(right.rule.code()))
    });
    Ok(Analysis {
        summary: Summary::from_diagnostics(files.len(), &diagnostics),
        diagnostics,
    })
}

pub fn collect_rust_files(paths: &[PathBuf], config: &Config) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_file() {
            if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path.clone());
            }
        } else if path.is_dir() {
            collect_directory(path, config, &mut files)?;
        } else {
            // traci: allow - the caller records this input failure at the CLI boundary.
            return Err(format!("path does not exist: {}", path.display()));
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn collect_directory(
    directory: &Path,
    config: &Config,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("cannot read {}: {error}", directory.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot read directory entry: {error}"))?;
        let path = entry.path();
        if config.path_ignored(&path) {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_directory(&path, config, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

pub fn analyze_source(path: &Path, source: &str, config: &Config) -> Vec<Diagnostic> {
    let raw_lines: Vec<&str> = source.lines().collect();
    let code_lines = sanitize_source(source);
    let test_lines = test_module_lines(&code_lines);
    let mut diagnostics = Vec::new();
    let mut emitted = HashSet::new();

    for (index, code) in code_lines.iter().enumerate() {
        let trimmed = code.trim();
        if trimmed.is_empty() || test_lines.contains(&index) || allows_line(&raw_lines, index) {
            continue;
        }

        let error_path = code.contains("return Err(")
            || trimmed.starts_with("Err(")
            || code.contains("=> Err(")
            || code.contains("if let Err(")
            || code.contains("Err(") && code.contains("=>");
        if error_path
            && config.rule_enabled(Rule::UntracedError)
            && !has_trace_nearby(&code_lines, index, config.trace_window)
        {
            push_diagnostic(
                &mut diagnostics,
                &mut emitted,
                path,
                &raw_lines,
                index,
                Rule::UntracedError,
                "failure path has no nearby structured diagnostic event",
            );
        }

        if config.rule_enabled(Rule::SwallowedResult)
            && (code.contains(".ok()") || is_discarded_assignment(trimmed))
        {
            push_diagnostic(
                &mut diagnostics,
                &mut emitted,
                path,
                &raw_lines,
                index,
                Rule::SwallowedResult,
                "result is discarded without a diagnostic record",
            );
        }

        if is_event_macro(code) {
            let event = statement_window(&raw_lines, index, 8);
            if config.rule_enabled(Rule::GenericMessage) && has_generic_message(&event) {
                push_diagnostic(
                    &mut diagnostics,
                    &mut emitted,
                    path,
                    &raw_lines,
                    index,
                    Rule::GenericMessage,
                    "diagnostic message is too generic to distinguish this failure",
                );
            }
            if config.rule_enabled(Rule::UnstructuredEvent) && !has_structured_field(&event) {
                push_diagnostic(
                    &mut diagnostics,
                    &mut emitted,
                    path,
                    &raw_lines,
                    index,
                    Rule::UnstructuredEvent,
                    "event has no structured contextual fields",
                );
            }
        }

        if config.rule_enabled(Rule::DetachedAsyncTask) && is_spawn(code) {
            let statement = statement_window(&raw_lines, index, 16);
            if !statement.contains(".instrument(")
                && !statement.contains("in_current_span(")
                && !statement.contains("Instrument::")
            {
                push_diagnostic(
                    &mut diagnostics,
                    &mut emitted,
                    path,
                    &raw_lines,
                    index,
                    Rule::DetachedAsyncTask,
                    "spawned task does not explicitly preserve trace context",
                );
            }
        }

        if config.rule_enabled(Rule::DiscardedErrorContext) && code.contains("map_err(|_|") {
            push_diagnostic(
                &mut diagnostics,
                &mut emitted,
                path,
                &raw_lines,
                index,
                Rule::DiscardedErrorContext,
                "error mapping discards the source error",
            );
        }

        if config.rule_enabled(Rule::OpaquePanic)
            && (code.contains(".unwrap()") || code.contains(".expect("))
        {
            push_diagnostic(
                &mut diagnostics,
                &mut emitted,
                path,
                &raw_lines,
                index,
                Rule::OpaquePanic,
                "panic path cannot be reconstructed from telemetry",
            );
        }

        if config.require_correlation_id
            && config.rule_enabled(Rule::MissingCorrelation)
            && is_public_async_function(trimmed)
        {
            let signature = statement_window(&raw_lines, index, 8);
            if !config
                .correlation_fields
                .iter()
                .any(|field| signature.contains(field))
            {
                push_diagnostic(
                    &mut diagnostics,
                    &mut emitted,
                    path,
                    &raw_lines,
                    index,
                    Rule::MissingCorrelation,
                    "public async boundary exposes no configured correlation field",
                );
            }
        }
    }
    diagnostics
}

fn sanitize_code(line: &str) -> String {
    sanitize_source(line).join("\n")
}

fn sanitize_source(source: &str) -> Vec<String> {
    #[derive(Default)]
    struct State {
        block_comment_depth: usize,
        in_string: bool,
        escaped: bool,
        raw_hashes: Option<usize>,
    }

    let mut state = State::default();
    let mut output = Vec::new();
    for line in source.lines() {
        let bytes = line.as_bytes();
        let mut sanitized = String::with_capacity(line.len());
        let mut index = 0;
        while index < bytes.len() {
            if let Some(hashes) = state.raw_hashes {
                let closes = bytes[index] == b'"'
                    && bytes
                        .get(index + 1..index + 1 + hashes)
                        .is_some_and(|tail| tail.iter().all(|byte| *byte == b'#'));
                sanitized.push(' ');
                index += 1;
                if closes {
                    sanitized.extend(std::iter::repeat(' ').take(hashes));
                    index += hashes;
                    state.raw_hashes = None;
                }
                continue;
            }
            if state.block_comment_depth > 0 {
                if bytes.get(index..index + 2) == Some(b"/*") {
                    state.block_comment_depth += 1;
                    sanitized.push_str("  ");
                    index += 2;
                } else if bytes.get(index..index + 2) == Some(b"*/") {
                    state.block_comment_depth -= 1;
                    sanitized.push_str("  ");
                    index += 2;
                } else {
                    sanitized.push(' ');
                    index += 1;
                }
                continue;
            }
            if state.in_string {
                sanitized.push(' ');
                if state.escaped {
                    state.escaped = false;
                } else if bytes[index] == b'\\' {
                    state.escaped = true;
                } else if bytes[index] == b'"' {
                    state.in_string = false;
                }
                index += 1;
                continue;
            }
            if bytes.get(index..index + 2) == Some(b"//") {
                break;
            }
            if bytes.get(index..index + 2) == Some(b"/*") {
                state.block_comment_depth = 1;
                sanitized.push_str("  ");
                index += 2;
                continue;
            }
            if bytes[index] == b'r' {
                let mut cursor = index + 1;
                while bytes.get(cursor) == Some(&b'#') {
                    cursor += 1;
                }
                if bytes.get(cursor) == Some(&b'"') {
                    let hashes = cursor - index - 1;
                    sanitized.extend(std::iter::repeat(' ').take(hashes + 2));
                    index = cursor + 1;
                    state.raw_hashes = Some(hashes);
                    continue;
                }
            }
            if bytes[index] == b'"' {
                state.in_string = true;
                sanitized.push(' ');
                index += 1;
                continue;
            }
            sanitized.push(char::from(bytes[index]));
            index += 1;
        }
        output.push(sanitized);
    }
    output
}

fn allows_line(lines: &[&str], index: usize) -> bool {
    lines[index].contains("traci: allow") || index > 0 && lines[index - 1].contains("traci: allow")
}

fn has_trace_nearby(lines: &[String], index: usize, window: usize) -> bool {
    let start = index.saturating_sub(window);
    let end = (index + 2).min(lines.len().saturating_sub(1));
    lines[start..=end].iter().any(|line| {
        is_event_macro(line)
            || line.contains("eprintln!(")
            || line.contains("record_error(")
            || line.contains(".record(")
            || line.contains("set_status(")
    })
}

fn test_module_lines(lines: &[String]) -> HashSet<usize> {
    let mut skipped = HashSet::new();
    let mut depth = 0_i32;
    let mut pending_test = false;
    let mut test_base = None;
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("#[cfg(test)]") {
            pending_test = true;
            skipped.insert(index);
        }
        if pending_test && trimmed.contains("mod tests") && trimmed.contains('{') {
            test_base = Some(depth);
            pending_test = false;
        }
        if test_base.is_some() {
            skipped.insert(index);
        }
        let opens = line.chars().filter(|character| *character == '{').count() as i32;
        let closes = line.chars().filter(|character| *character == '}').count() as i32;
        depth += opens - closes;
        if test_base.is_some_and(|base| depth <= base) {
            test_base = None;
        }
    }
    skipped
}

fn is_event_macro(code: &str) -> bool {
    [
        "trace!(", "debug!(", "info!(", "warn!(", "error!(", "event!(",
    ]
    .iter()
    .any(|needle| code.contains(needle))
}

fn is_discarded_assignment(trimmed: &str) -> bool {
    trimmed
        .strip_prefix("let _ =")
        .and_then(|value| value.strip_suffix(';'))
        .is_some_and(|value| {
            let expression = value.trim();
            expression.contains('(') || expression.contains(".await") || expression.contains('?')
        })
}

fn is_spawn(code: &str) -> bool {
    code.contains("tokio::spawn(") || code.contains("task::spawn(") || code.contains("spawn(async")
}

fn is_public_async_function(trimmed: &str) -> bool {
    trimmed.starts_with("pub async fn ")
        || trimmed.starts_with("pub(crate) async fn ")
        || trimmed.starts_with("pub(super) async fn ")
}

fn statement_window(lines: &[&str], start: usize, maximum: usize) -> String {
    let mut statement = String::new();
    let end = (start + maximum).min(lines.len());
    for line in &lines[start..end] {
        statement.push_str(line);
        statement.push('\n');
        if line.contains(");") || line.contains(" {") {
            break;
        }
    }
    statement
}

fn has_structured_field(event: &str) -> bool {
    let code = sanitize_code(event);
    code.contains(" = ") || code.contains("=?") || code.contains("=%") || code.contains("field::")
}

fn has_generic_message(event: &str) -> bool {
    let lowercase = event.to_ascii_lowercase();
    [
        "\"error\"",
        "\"failed\"",
        "\"operation failed\"",
        "\"something went wrong\"",
        "\"unexpected error\"",
        "\"unknown error\"",
    ]
    .iter()
    .any(|message| lowercase.contains(message))
}

#[allow(clippy::too_many_arguments)]
fn push_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    emitted: &mut HashSet<(usize, &'static str)>,
    path: &Path,
    lines: &[&str],
    index: usize,
    rule: Rule,
    message: &str,
) {
    if !emitted.insert((index, rule.code())) {
        return;
    }
    let snippet = lines.get(index).copied().unwrap_or("").trim().to_string();
    let column = lines
        .get(index)
        .map_or(1, |line| line.len() - line.trim_start().len() + 1);
    diagnostics.push(Diagnostic {
        rule,
        severity: rule.default_severity(),
        path: path.to_path_buf(),
        line: index + 1,
        column,
        message: message.to_string(),
        snippet,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(source: &str) -> Vec<Rule> {
        analyze_source(Path::new("sample.rs"), source, &Config::default())
            .into_iter()
            .map(|diagnostic| diagnostic.rule)
            .collect()
    }

    #[test]
    fn catches_untraced_error() {
        assert!(rules("fn load() {\n    return Err(DbError);\n}").contains(&Rule::UntracedError));
    }

    #[test]
    fn accepts_structured_traced_error() {
        let findings = rules(
            r#"fn load(id: u64) {
                error!(operation = "load", customer_id = id, "database timeout");
                return Err(DbError);
            }"#,
        );
        assert!(!findings.contains(&Rule::UntracedError));
        assert!(!findings.contains(&Rule::UnstructuredEvent));
    }

    #[test]
    fn catches_swallowed_result_and_panic() {
        let findings = rules("fn run() { save().ok(); parse().unwrap(); }");
        assert!(findings.contains(&Rule::SwallowedResult));
        assert!(findings.contains(&Rule::OpaquePanic));
    }

    #[test]
    fn accepts_deliberately_ignored_plain_value() {
        assert!(!rules("fn run(customer_id: u64) { let _ = customer_id; }")
            .contains(&Rule::SwallowedResult));
    }

    #[test]
    fn catches_uninstrumented_spawn() {
        assert!(
            rules("fn run() { tokio::spawn(async move { work().await; }); }")
                .contains(&Rule::DetachedAsyncTask)
        );
    }

    #[test]
    fn ignores_patterns_inside_strings_and_comments() {
        assert!(rules(
            r#"fn docs() { let text = ".unwrap()"; // return Err(X)
        }"#
        )
        .is_empty());
    }

    #[test]
    fn enforces_correlation_on_public_async_boundaries() {
        let config = Config {
            require_correlation_id: true,
            ..Config::default()
        };
        let findings = analyze_source(
            Path::new("api.rs"),
            "pub async fn handle(user_id: u64) -> Result<(), Error> { Ok(()) }",
            &config,
        );
        assert!(findings
            .iter()
            .any(|finding| finding.rule == Rule::MissingCorrelation));
    }
}
