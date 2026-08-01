use crate::analyzer::Analysis;
use crate::complexity::ComplexityReport;
use crate::trace::{TraceOutcome, TracePlan};
use crate::types::{ColorChoice, Severity};
use std::env;
use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const MAGENTA: &str = "\x1b[35m";
const CYAN: &str = "\x1b[36m";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Style {
    pub color: bool,
    pub emoji: bool,
    pub interactive: bool,
}

impl Style {
    #[must_use]
    pub fn stdout(choice: ColorChoice, emoji: bool) -> Self {
        let terminal = io::stdout().is_terminal();
        Self {
            color: color_enabled(choice, terminal),
            emoji,
            interactive: terminal,
        }
    }

    #[must_use]
    pub const fn plain() -> Self {
        Self {
            color: false,
            emoji: false,
            interactive: false,
        }
    }
}

#[must_use]
pub fn color_enabled(choice: ColorChoice, terminal: bool) -> bool {
    match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => {
            terminal
                && env::var_os("NO_COLOR").is_none()
                && env::var("TERM").map_or(true, |term| term != "dumb")
        }
    }
}

#[must_use]
pub fn stderr_is_terminal() -> bool {
    io::stderr().is_terminal()
}

#[must_use]
pub fn render_human(analysis: &Analysis, style: Style, scope: &str) -> String {
    let mut output = String::new();
    let lens = icon(style, "🔭", "TRACI");
    output.push_str(&paint(
        &format!(
            "{lens}  OBSERVABILITY GATE · {}\n",
            scope.to_ascii_uppercase()
        ),
        CYAN,
        style.color,
    ));
    if analysis.diagnostics.is_empty() {
        output.push_str(&paint(
            &format!(
                "{}  All inspected execution paths are observable.\n",
                icon(style, "✅", "OK")
            ),
            GREEN,
            style.color,
        ));
    }
    for diagnostic in &analysis.diagnostics {
        let marker = severity_icon(style, diagnostic.severity);
        let heading = format!(
            "{marker}  {:8} {}  {}:{}:{}\n",
            diagnostic.severity.to_string().to_ascii_uppercase(),
            diagnostic.rule.code(),
            diagnostic.path.display(),
            diagnostic.line,
            diagnostic.column
        );
        output.push_str(&paint(
            &heading,
            severity_color(diagnostic.severity),
            style.color,
        ));
        output.push_str(&format!("   {}\n", diagnostic.message));
        output.push_str(&paint(
            &format!("   │ {}\n", diagnostic.snippet),
            DIM,
            style.color,
        ));
        output.push_str(&format!(
            "   {} {}\n",
            icon(style, "💡", "help:"),
            diagnostic.rule.description()
        ));
    }
    output.push_str(&summary_line(analysis, style));
    output
}

#[must_use]
pub fn render_dashboard(analysis: &Analysis, style: Style, scope: &str) -> String {
    let width = terminal_width();
    let mut output = String::new();
    if style.interactive {
        output.push_str("\x1b[2J\x1b[H");
    }
    let status = if analysis.diagnostics.is_empty() {
        format!("{} POLICY HEALTHY", icon(style, "✅", "OK"))
    } else {
        format!("{} ATTENTION REQUIRED", icon(style, "🚨", "ALERT"))
    };
    output.push_str(&paint(&top_border(width), CYAN, style.color));
    output.push('\n');
    output.push_str(&box_row(
        &format!(
            "{} TRACI  ·  OBSERVABILITY CONTROL  ·  {}",
            icon(style, "🔭", ""),
            scope.to_ascii_uppercase()
        ),
        width,
    ));
    output.push('\n');
    output.push_str(&paint(&divider(width), CYAN, style.color));
    output.push('\n');
    let status_row = box_row(&status, width);
    output.push_str(&paint(
        &status_row,
        if analysis.diagnostics.is_empty() {
            GREEN
        } else {
            RED
        },
        style.color,
    ));
    output.push('\n');
    let summary = analysis.summary;
    output.push_str(&box_row(
        &format!(
            "FILES {:>4}   FINDINGS {:>4}   INFO {:>3}   WARN {:>3}   ERROR {:>3}   CRITICAL {:>3}",
            summary.files,
            summary.diagnostics,
            summary.info,
            summary.warnings,
            summary.errors,
            summary.critical
        ),
        width,
    ));
    output.push('\n');
    output.push_str(&box_row(
        &format!(
            "RISK  {}",
            risk_bar(
                summary.critical * 4 + summary.errors * 2 + summary.warnings,
                28
            )
        ),
        width,
    ));
    output.push('\n');
    output.push_str(&paint(&divider(width), CYAN, style.color));
    output.push('\n');
    output.push_str(&box_row(
        &format!("{} FINDINGS", icon(style, "🧭", "")),
        width,
    ));
    output.push('\n');
    let limit = terminal_height().saturating_sub(12).max(3);
    if analysis.diagnostics.is_empty() {
        output.push_str(&box_row("No observability dead zones detected.", width));
        output.push('\n');
    } else {
        for diagnostic in analysis.diagnostics.iter().take(limit) {
            let line = format!(
                "{} {:8} {}  {}:{}  {}",
                severity_icon(style, diagnostic.severity),
                diagnostic.severity.to_string().to_ascii_uppercase(),
                diagnostic.rule.code(),
                diagnostic.path.display(),
                diagnostic.line,
                diagnostic.message
            );
            let row = box_row(&line, width);
            output.push_str(&paint(
                &row,
                severity_color(diagnostic.severity),
                style.color,
            ));
            output.push('\n');
        }
        let hidden = analysis.diagnostics.len().saturating_sub(limit);
        if hidden > 0 {
            output.push_str(&box_row(&format!("… {hidden} more finding(s)"), width));
            output.push('\n');
        }
    }
    output.push_str(&paint(&divider(width), CYAN, style.color));
    output.push('\n');
    output.push_str(&box_row(
        &format!(
            "{} live dashboard   scope: {scope}   Ctrl-C: quit",
            icon(style, "✨", "*")
        ),
        width,
    ));
    output.push('\n');
    output.push_str(&paint(&bottom_border(width), CYAN, style.color));
    output.push('\n');
    output
}

#[must_use]
pub fn render_complexity(report: &ComplexityReport, style: Style, scope: &str) -> String {
    let mut output = paint(
        &format!(
            "{}  TRACE IMPLEMENTATION COMPLEXITY · {}\n",
            icon(style, "🧠", "TRACI"),
            scope.to_ascii_uppercase()
        ),
        MAGENTA,
        style.color,
    );
    let score_color = if report.score >= 70 {
        RED
    } else if report.score >= 35 {
        YELLOW
    } else {
        GREEN
    };
    output.push_str(&paint(
        &format!("{:>3}/100  {}\n", report.score, score_bar(report.score, 30)),
        score_color,
        style.color,
    ));
    output.push_str(&format!(
        "{} files · {} lines · {} functions · {} branches · {} error paths · {} async · {} concurrent · nesting {}\n",
        report.files.len(),
        report.lines,
        report.functions,
        report.branches,
        report.error_paths,
        report.async_points,
        report.concurrency_points,
        report.max_nesting
    ));
    if !report.files.is_empty() {
        output.push_str(&paint("\nHOTTEST FILES\n", BOLD, style.color));
    }
    for file in report.files.iter().take(10) {
        output.push_str(&format!(
            "  {:>3}  {}  {:>5} lines  {}\n",
            file.score,
            score_bar(file.score, 12),
            file.lines,
            file.path.display()
        ));
    }
    output
}

#[must_use]
pub fn render_trace_plan(plan: &TracePlan, style: Style) -> String {
    let title = format!("{}  GUARDED TRACE PLAN\n", icon(style, "🤖", "TRACI"));
    format!(
        "{}  goal        {}\n  complexity  {}/100 {}\n  provider    {}\n  model       {}\n  branch      {}\n  apply       {}\n",
        paint(&title, BLUE, style.color),
        plan.goal,
        plan.complexity_score,
        score_bar(plan.complexity_score, 16),
        plan.provider,
        plan.model,
        plan.branch,
        if plan.apply { icon(style, "⚡ yes", "yes") } else { icon(style, "🛡️ no (plan only)", "no (plan only)") }
    )
}

#[must_use]
pub fn render_trace_outcome(outcome: &TraceOutcome, style: Style) -> String {
    let (marker, label, color) = if outcome.merged {
        (icon(style, "✅", "OK"), "MERGED", GREEN)
    } else {
        (icon(style, "⛔", "NO"), "REJECTED", RED)
    };
    paint(
        &format!(
            "{marker}  TRACE {label}\n  branch  {}\n  {}\n",
            outcome.plan.branch, outcome.reason
        ),
        color,
        style.color,
    )
}

#[must_use]
pub fn notice(message: &str, style: Style, success: bool) -> String {
    let marker = if success {
        icon(style, "✅", "OK")
    } else {
        icon(style, "ℹ️", "INFO")
    };
    paint(
        &format!("{marker}  {message}"),
        if success { GREEN } else { BLUE },
        style.color,
    )
}

pub struct Spinner {
    running: Option<Arc<AtomicBool>>,
    handle: Option<JoinHandle<()>>,
}

impl Spinner {
    #[must_use]
    pub fn start(enabled: bool, label: &str, style: Style) -> Self {
        if !enabled {
            return Self {
                running: None,
                handle: None,
            };
        }
        let running = Arc::new(AtomicBool::new(true));
        let worker_running = Arc::clone(&running);
        let label = label.to_string();
        let handle = thread::spawn(move || {
            let frames = if style.emoji {
                ["◐", "◓", "◑", "◒"]
            } else {
                ["|", "/", "-", "\\"]
            };
            let mut index = 0;
            while worker_running.load(Ordering::Relaxed) {
                let frame = paint(frames[index % frames.len()], CYAN, style.color);
                eprint!("\r\x1b[2K{frame}  {label}");
                let _ignored = io::stderr().flush();
                index += 1;
                thread::sleep(Duration::from_millis(80));
            }
            eprint!("\r\x1b[2K");
            let _ignored = io::stderr().flush();
        });
        Self {
            running: Some(running),
            handle: Some(handle),
        }
    }

    pub fn finish(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        if let Some(running) = self.running.take() {
            running.store(false, Ordering::Relaxed);
        }
        if let Some(handle) = self.handle.take() {
            let _ignored = handle.join();
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.stop();
    }
}

fn summary_line(analysis: &Analysis, style: Style) -> String {
    let summary = analysis.summary;
    let marker = if summary.diagnostics == 0 {
        icon(style, "✨", "SUMMARY")
    } else {
        icon(style, "📊", "SUMMARY")
    };
    paint(
        &format!(
            "{marker}  {} file(s) · {} finding(s) · {} info · {} warning · {} error · {} critical\n",
            summary.files,
            summary.diagnostics,
            summary.info,
            summary.warnings,
            summary.errors,
            summary.critical
        ),
        if summary.diagnostics == 0 { GREEN } else { CYAN },
        style.color,
    )
}

fn icon(style: Style, emoji: &'static str, fallback: &'static str) -> &'static str {
    if style.emoji {
        emoji
    } else {
        fallback
    }
}

fn severity_icon(style: Style, severity: Severity) -> &'static str {
    if !style.emoji {
        return match severity {
            Severity::Info => "i",
            Severity::Warning => "!",
            Severity::Error => "x",
            Severity::Critical => "!!",
        };
    }
    match severity {
        Severity::Info => "💬",
        Severity::Warning => "⚠️",
        Severity::Error => "❌",
        Severity::Critical => "💀",
    }
}

fn severity_color(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => BLUE,
        Severity::Warning => YELLOW,
        Severity::Error => RED,
        Severity::Critical => MAGENTA,
    }
}

fn paint(value: &str, color: &str, enabled: bool) -> String {
    if enabled {
        format!("{color}{value}{RESET}")
    } else {
        value.to_string()
    }
}

fn terminal_width() -> usize {
    environment_dimension("COLUMNS", 96).clamp(64, 140)
}

fn terminal_height() -> usize {
    environment_dimension("LINES", 28).clamp(16, 80)
}

fn environment_dimension(name: &str, fallback: usize) -> usize {
    match env::var(name) {
        Ok(value) => match value.parse() {
            Ok(parsed) => parsed,
            // traci: allow - malformed optional terminal hints safely use the documented fallback.
            Err(_) => fallback,
        },
        // traci: allow - absent optional terminal hints safely use the documented fallback.
        Err(_) => fallback,
    }
}

fn top_border(width: usize) -> String {
    format!("╭{}╮", "─".repeat(width.saturating_sub(2)))
}

fn divider(width: usize) -> String {
    format!("├{}┤", "─".repeat(width.saturating_sub(2)))
}

fn bottom_border(width: usize) -> String {
    format!("╰{}╯", "─".repeat(width.saturating_sub(2)))
}

fn box_row(value: &str, width: usize) -> String {
    let available = width.saturating_sub(4);
    let compact = value.replace(['\n', '\r'], " ");
    let text = truncate(&compact, available);
    let padding = available.saturating_sub(text.chars().count());
    format!("│ {text}{} │", " ".repeat(padding))
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }
    let keep = width.saturating_sub(1);
    format!("{}…", value.chars().take(keep).collect::<String>())
}

fn risk_bar(risk: usize, width: usize) -> String {
    let filled = risk.min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

fn score_bar(score: u8, width: usize) -> String {
    let filled = usize::from(score) * width / 100;
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Summary;

    #[test]
    fn dashboard_is_readable_without_terminal_controls() {
        let analysis = Analysis {
            diagnostics: Vec::new(),
            summary: Summary {
                files: 3,
                ..Summary::default()
            },
        };
        let output = render_dashboard(&analysis, Style::plain(), "production");
        assert!(output.contains("OBSERVABILITY CONTROL"));
        assert!(output.contains("PRODUCTION"));
        assert!(output.contains("FILES"));
        assert!(!output.contains("\x1b["));
    }

    #[test]
    fn no_color_disables_auto_color() {
        assert!(!color_enabled(ColorChoice::Auto, false));
        assert!(color_enabled(ColorChoice::Always, false));
        assert!(!color_enabled(ColorChoice::Never, true));
    }
}
