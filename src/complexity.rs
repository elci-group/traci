use crate::analyzer::collect_rust_files;
use crate::config::Config;
use crate::report::{escape_json, Format};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileComplexity {
    pub path: PathBuf,
    pub lines: usize,
    pub functions: usize,
    pub branches: usize,
    pub error_paths: usize,
    pub async_points: usize,
    pub concurrency_points: usize,
    pub max_nesting: usize,
    pub score: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComplexityReport {
    pub files: Vec<FileComplexity>,
    pub lines: usize,
    pub functions: usize,
    pub branches: usize,
    pub error_paths: usize,
    pub async_points: usize,
    pub concurrency_points: usize,
    pub max_nesting: usize,
    pub score: u8,
}

pub fn estimate_paths(paths: &[PathBuf], config: &Config) -> Result<ComplexityReport, String> {
    let files = collect_rust_files(paths, config)?;
    let mut reports = Vec::with_capacity(files.len());
    for path in files {
        let source = fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        reports.push(estimate_source(&path, &source));
    }
    Ok(summarize(reports))
}

#[must_use]
pub fn estimate_source(path: &Path, source: &str) -> FileComplexity {
    let mut functions = 0;
    let mut branches = 0;
    let mut error_paths = 0;
    let mut async_points = 0;
    let mut concurrency_points = 0;
    let mut depth = 0_usize;
    let mut max_nesting = 0;
    let mut in_block_comment = false;

    for raw_line in source.lines() {
        let line = visible_code(raw_line, &mut in_block_comment);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        functions += occurrences(trimmed, "fn ");
        branches += occurrences(trimmed, "if ")
            + occurrences(trimmed, "match ")
            + occurrences(trimmed, "for ")
            + occurrences(trimmed, "while ")
            + occurrences(trimmed, "&&")
            + occurrences(trimmed, "||")
            + occurrences(trimmed, "=>");
        error_paths += occurrences(trimmed, "Err(") + occurrences(trimmed, '?');
        async_points += occurrences(trimmed, "async ") + occurrences(trimmed, ".await");
        concurrency_points += occurrences(trimmed, "spawn(")
            + occurrences(trimmed, "Mutex")
            + occurrences(trimmed, "RwLock")
            + occurrences(trimmed, "channel(");
        for character in trimmed.chars() {
            match character {
                '{' => {
                    depth += 1;
                    max_nesting = max_nesting.max(depth);
                }
                '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }

    let lines = source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    let score = score_metrics(
        lines,
        functions,
        branches,
        error_paths,
        async_points,
        concurrency_points,
        max_nesting,
        1,
    );
    FileComplexity {
        path: path.to_path_buf(),
        lines,
        functions,
        branches,
        error_paths,
        async_points,
        concurrency_points,
        max_nesting,
        score,
    }
}

fn summarize(mut files: Vec<FileComplexity>) -> ComplexityReport {
    files.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then(left.path.cmp(&right.path))
    });
    let lines = files.iter().map(|file| file.lines).sum();
    let functions = files.iter().map(|file| file.functions).sum();
    let branches = files.iter().map(|file| file.branches).sum();
    let error_paths = files.iter().map(|file| file.error_paths).sum();
    let async_points = files.iter().map(|file| file.async_points).sum();
    let concurrency_points = files.iter().map(|file| file.concurrency_points).sum();
    let max_nesting = files.iter().map(|file| file.max_nesting).max().unwrap_or(0);
    let score = score_metrics(
        lines,
        functions,
        branches,
        error_paths,
        async_points,
        concurrency_points,
        max_nesting,
        files.len(),
    );
    ComplexityReport {
        files,
        lines,
        functions,
        branches,
        error_paths,
        async_points,
        concurrency_points,
        max_nesting,
        score,
    }
}

fn score_metrics(
    lines: usize,
    functions: usize,
    branches: usize,
    error_paths: usize,
    async_points: usize,
    concurrency_points: usize,
    max_nesting: usize,
    files: usize,
) -> u8 {
    let points = (lines / 100).min(20)
        + (functions / 3).min(10)
        + (branches / 2).min(20)
        + (error_paths * 2).min(15)
        + (async_points * 2).min(10)
        + (concurrency_points * 3).min(10)
        + (max_nesting * 2).min(10)
        + files.min(5);
    u8::try_from(points.min(100)).unwrap_or(100)
}

fn occurrences(source: &str, needle: impl Pattern) -> usize {
    needle.count_in(source)
}

trait Pattern {
    fn count_in(self, source: &str) -> usize;
}

impl Pattern for &str {
    fn count_in(self, source: &str) -> usize {
        source.matches(self).count()
    }
}

impl Pattern for char {
    fn count_in(self, source: &str) -> usize {
        source.matches(self).count()
    }
}

fn visible_code(line: &str, in_block_comment: &mut bool) -> String {
    let mut output = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut in_string = false;
    while let Some(character) = chars.next() {
        if *in_block_comment {
            if character == '*' && chars.peek() == Some(&'/') {
                chars.next();
                *in_block_comment = false;
            }
            continue;
        }
        if !in_string && character == '/' && chars.peek() == Some(&'*') {
            chars.next();
            *in_block_comment = true;
            continue;
        }
        if !in_string && character == '/' && chars.peek() == Some(&'/') {
            break;
        }
        if character == '"' {
            in_string = !in_string;
            output.push(' ');
        } else if in_string {
            output.push(' ');
        } else {
            output.push(character);
        }
    }
    output
}

#[must_use]
pub fn render_complexity(report: &ComplexityReport, format: Format) -> String {
    match format {
        Format::Text => render_human(report),
        Format::Json | Format::Jsonl => render_json(report),
        Format::Sarif => render_json(report),
    }
}

fn render_human(report: &ComplexityReport) -> String {
    let mut output = format!(
        "Traci complexity: {}/100 across {} Rust file(s)\n\
         metrics: {} lines, {} functions, {} branches, {} error paths, {} async points, {} concurrency points, nesting {}\n",
        report.score,
        report.files.len(),
        report.lines,
        report.functions,
        report.branches,
        report.error_paths,
        report.async_points,
        report.concurrency_points,
        report.max_nesting
    );
    for file in report.files.iter().take(10) {
        output.push_str(&format!(
            "  {:3}/100  {:5} lines  {}\n",
            file.score,
            file.lines,
            file.path.display()
        ));
    }
    output
}

fn render_json(report: &ComplexityReport) -> String {
    let files = report
        .files
        .iter()
        .map(|file| {
            format!(
                "{{\"path\":\"{}\",\"score\":{},\"lines\":{},\"functions\":{},\"branches\":{},\"error_paths\":{},\"async_points\":{},\"concurrency_points\":{},\"max_nesting\":{}}}",
                escape_json(&file.path.to_string_lossy()),
                file.score,
                file.lines,
                file.functions,
                file.branches,
                file.error_paths,
                file.async_points,
                file.concurrency_points,
                file.max_nesting
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"score\":{},\"files_count\":{},\"lines\":{},\"functions\":{},\"branches\":{},\"error_paths\":{},\"async_points\":{},\"concurrency_points\":{},\"max_nesting\":{},\"files\":[{files}]}}",
        report.score,
        report.files.len(),
        report.lines,
        report.functions,
        report.branches,
        report.error_paths,
        report.async_points,
        report.concurrency_points,
        report.max_nesting
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_branching_code_scores_above_plain_code() {
        let plain = estimate_source(Path::new("plain.rs"), "fn value() -> u8 { 1 }");
        let complex = estimate_source(
            Path::new("complex.rs"),
            "async fn load() { if ready() { tokio::spawn(async { work().await }); } else { return Err(Failed); } }",
        );
        assert!(complex.score > plain.score);
        assert!(complex.async_points > 0);
        assert!(complex.concurrency_points > 0);
    }

    #[test]
    fn json_contains_machine_metrics() {
        let report = summarize(vec![estimate_source(Path::new("a.rs"), "fn a() {}")]);
        let output = render_complexity(&report, Format::Json);
        assert!(output.contains("\"score\""));
        assert!(output.contains("\"files_count\":1"));
    }
}
