use crate::analyzer::Analysis;
use crate::types::Diagnostic;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    Text,
    Json,
    Jsonl,
    Sarif,
}

impl Format {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "text" | "human" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            "jsonl" | "ndjson" => Ok(Self::Jsonl),
            "sarif" => Ok(Self::Sarif),
            // traci: allow - main records this option error with structured context.
            other => Err(format!("unknown output format '{other}'")),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "human",
            Self::Json => "json",
            Self::Jsonl => "jsonl",
            Self::Sarif => "sarif",
        }
    }
}

pub fn render(analysis: &Analysis, format: Format) -> String {
    match format {
        Format::Text => render_text(analysis),
        Format::Json => render_json(analysis),
        Format::Jsonl => render_jsonl(analysis),
        Format::Sarif => render_sarif(analysis),
    }
}

fn render_jsonl(analysis: &Analysis) -> String {
    let mut lines = analysis
        .diagnostics
        .iter()
        .map(|diagnostic| {
            format!(
                "{{\"type\":\"diagnostic\",\"data\":{}}}",
                diagnostic_json(diagnostic)
            )
        })
        .collect::<Vec<_>>();
    let summary = analysis.summary;
    lines.push(format!(
        "{{\"type\":\"summary\",\"files\":{},\"diagnostics\":{},\"info\":{},\"warnings\":{},\"errors\":{},\"critical\":{}}}",
        summary.files,
        summary.diagnostics,
        summary.info,
        summary.warnings,
        summary.errors,
        summary.critical
    ));
    format!("{}\n", lines.join("\n"))
}

fn render_text(analysis: &Analysis) -> String {
    let mut output = String::new();
    for diagnostic in &analysis.diagnostics {
        let location = format!(
            "{}:{}:{}: {} [{}] {}\n",
            diagnostic.path.display(),
            diagnostic.line,
            diagnostic.column,
            diagnostic.severity,
            diagnostic.rule.code(),
            diagnostic.message
        );
        output.push_str(&location);
        let snippet = format!("  | {}\n", diagnostic.snippet);
        output.push_str(&snippet);
        let help = format!("  = help: {}\n", diagnostic.rule.description());
        output.push_str(&help);
    }
    let summary = analysis.summary;
    let totals = format!(
        "Traci: {} file(s), {} finding(s) ({} info, {} warning, {} error, {} critical)\n",
        summary.files,
        summary.diagnostics,
        summary.info,
        summary.warnings,
        summary.errors,
        summary.critical
    );
    output.push_str(&totals);
    output
}

fn render_json(analysis: &Analysis) -> String {
    let diagnostics = analysis
        .diagnostics
        .iter()
        .map(diagnostic_json)
        .collect::<Vec<_>>()
        .join(",");
    let summary = analysis.summary;
    format!(
        "{{\"diagnostics\":[{diagnostics}],\"summary\":{{\"files\":{},\"diagnostics\":{},\"info\":{},\"warnings\":{},\"errors\":{},\"critical\":{}}}}}",
        summary.files,
        summary.diagnostics,
        summary.info,
        summary.warnings,
        summary.errors,
        summary.critical
    )
}

fn diagnostic_json(diagnostic: &Diagnostic) -> String {
    format!(
        "{{\"rule\":\"{}\",\"severity\":\"{}\",\"path\":\"{}\",\"line\":{},\"column\":{},\"message\":\"{}\",\"snippet\":\"{}\"}}",
        diagnostic.rule.code(),
        diagnostic.severity,
        escape_json(&diagnostic.path.to_string_lossy()),
        diagnostic.line,
        diagnostic.column,
        escape_json(&diagnostic.message),
        escape_json(&diagnostic.snippet)
    )
}

fn render_sarif(analysis: &Analysis) -> String {
    let results = analysis
        .diagnostics
        .iter()
        .map(|diagnostic| {
            format!(
                "{{\"ruleId\":\"{}\",\"level\":\"{}\",\"message\":{{\"text\":\"{}\"}},\"locations\":[{{\"physicalLocation\":{{\"artifactLocation\":{{\"uri\":\"{}\"}},\"region\":{{\"startLine\":{},\"startColumn\":{}}}}}}}]}}",
                diagnostic.rule.code(),
                sarif_level(diagnostic),
                escape_json(&diagnostic.message),
                escape_json(&diagnostic.path.to_string_lossy()),
                diagnostic.line,
                diagnostic.column
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"version\":\"2.1.0\",\"$schema\":\"https://json.schemastore.org/sarif-2.1.0.json\",\"runs\":[{{\"tool\":{{\"driver\":{{\"name\":\"Traci\",\"version\":\"{}\"}}}},\"results\":[{results}]}}]}}",
        env!("CARGO_PKG_VERSION")
    )
}

fn sarif_level(diagnostic: &Diagnostic) -> &'static str {
    use crate::types::Severity;
    match diagnostic.severity {
        Severity::Info => "note",
        Severity::Warning => "warning",
        Severity::Error | Severity::Critical => "error",
    }
}

pub(crate) fn escape_json(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                let encoded = format!("\\u{:04x}", character as u32);
                escaped.push_str(&encoded);
            }
            character => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::analyze_source;
    use crate::config::Config;
    use crate::types::Summary;
    use std::path::Path;

    #[test]
    fn emits_valid_shape_for_json() {
        let diagnostics = analyze_source(
            Path::new("quote.rs"),
            "fn f() { return Err(\"bad\"); }",
            &Config::default(),
        );
        let analysis = Analysis {
            summary: Summary::from_diagnostics(1, &diagnostics),
            diagnostics,
        };
        let output = render(&analysis, Format::Json);
        assert!(output.starts_with("{\"diagnostics\":["));
        assert!(output.contains("\\\"bad\\\""));
        assert!(output.ends_with('}'));
    }

    #[test]
    fn emits_sarif_envelope() {
        let analysis = Analysis {
            diagnostics: Vec::new(),
            summary: Summary::default(),
        };
        let output = render(&analysis, Format::Sarif);
        assert!(output.contains("\"version\":\"2.1.0\""));
        assert!(output.contains("\"name\":\"Traci\""));
    }
}
