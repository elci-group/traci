use crate::types::{Rule, Severity};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Clone, Debug)]
pub struct Config {
    pub fail_on: Severity,
    pub trace_window: usize,
    pub require_correlation_id: bool,
    pub correlation_fields: Vec<String>,
    pub ignore: Vec<String>,
    pub enabled_rules: HashSet<Rule>,
    pub watch_interval_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fail_on: Severity::Error,
            trace_window: 6,
            require_correlation_id: false,
            correlation_fields: vec![
                "correlation_id".into(),
                "request_id".into(),
                "trace_id".into(),
            ],
            ignore: vec!["target".into(), ".git".into(), ".kaptaind".into()],
            enabled_rules: Rule::ALL.into_iter().collect(),
            watch_interval_ms: 750,
        }
    }
}

impl Config {
    pub fn discover(start: &Path) -> Result<(Self, Option<PathBuf>), String> {
        let mut cursor = if start.is_file() {
            start.parent().unwrap_or(Path::new(".")).to_path_buf()
        } else {
            start.to_path_buf()
        };
        loop {
            let candidate = cursor.join("traci.toml");
            if candidate.is_file() {
                return Self::from_path(&candidate).map(|config| (config, Some(candidate)));
            }
            if !cursor.pop() {
                return Ok((Self::default(), None));
            }
        }
    }

    pub fn from_path(path: &Path) -> Result<Self, String> {
        let source = fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        Self::parse(&source).map_err(|error| format!("{}: {error}", path.display()))
    }

    pub fn parse(source: &str) -> Result<Self, String> {
        let mut config = Self::default();
        let mut section = String::new();
        for (index, raw_line) in source.lines().enumerate() {
            let line = raw_line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].trim().to_string();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                // traci: allow - the caller records policy failures at the CLI boundary.
                return Err(format!("line {}: expected key = value", index + 1));
            };
            let key = key.trim();
            let value = value.trim();
            match (section.as_str(), key) {
                ("analysis", "fail_on") => {
                    config.fail_on = Severity::from_str(&unquote(value)?)?;
                }
                ("analysis", "trace_window") => {
                    config.trace_window = parse_number(value, index + 1)?;
                }
                ("analysis", "require_correlation_id") => {
                    config.require_correlation_id = parse_bool(value, index + 1)?;
                }
                ("analysis", "correlation_fields") => {
                    config.correlation_fields = parse_string_array(value)?;
                }
                ("analysis", "ignore") => config.ignore = parse_string_array(value)?,
                ("analysis", "enabled_rules") => {
                    let codes = parse_string_array(value)?;
                    let mut rules = HashSet::new();
                    for code in codes {
                        let rule = Rule::from_code(&code)
                            .ok_or_else(|| format!("unknown rule code '{code}'"))?;
                        rules.insert(rule);
                    }
                    config.enabled_rules = rules;
                }
                ("watch", "interval_ms") => {
                    config.watch_interval_ms = parse_number(value, index + 1)?;
                }
                _ => {
                    // traci: allow - the caller records policy failures at the CLI boundary.
                    return Err(format!("line {}: unknown key {section}.{key}", index + 1));
                }
            }
        }
        if config.trace_window == 0 {
            // traci: allow - the caller records policy failures at the CLI boundary.
            return Err("analysis.trace_window must be greater than zero".into());
        }
        if config.watch_interval_ms < 50 {
            // traci: allow - the caller records policy failures at the CLI boundary.
            return Err("watch.interval_ms must be at least 50".into());
        }
        if config.correlation_fields.is_empty() {
            // traci: allow - the caller records policy failures at the CLI boundary.
            return Err("analysis.correlation_fields cannot be empty".into());
        }
        Ok(config)
    }

    pub fn rule_enabled(&self, rule: Rule) -> bool {
        self.enabled_rules.contains(&rule)
    }

    pub fn path_ignored(&self, path: &Path) -> bool {
        let normalized = path.to_string_lossy().replace('\\', "/");
        self.ignore.iter().any(|pattern| {
            normalized
                .split('/')
                .any(|component| component == pattern.trim_end_matches("/**"))
                || normalized.contains(pattern.trim_end_matches("/**"))
        })
    }
}

fn unquote(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        Ok(trimmed[1..trimmed.len() - 1].to_string())
    } else {
        // traci: allow - the caller records policy failures at the CLI boundary.
        Err(format!("expected quoted string, got '{trimmed}'"))
    }
}

fn parse_number<T>(value: &str, line: usize) -> Result<T, String>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    value
        .parse()
        .map_err(|error| format!("line {line}: expected a positive integer: {error}"))
}

fn parse_bool(value: &str, line: usize) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        // traci: allow - the caller records policy failures at the CLI boundary.
        _ => Err(format!("line {line}: expected true or false")),
    }
}

fn parse_string_array(value: &str) -> Result<Vec<String>, String> {
    let trimmed = value.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        // traci: allow - the caller records policy failures at the CLI boundary.
        return Err(format!("expected an array of strings, got '{trimmed}'"));
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    if inner.trim().is_empty() {
        return Ok(Vec::new());
    }
    inner.split(',').map(unquote).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_policy() {
        let config = Config::parse(
            r#"
                [analysis]
                fail_on = "critical"
                trace_window = 3
                require_correlation_id = true
                correlation_fields = ["request_id"]
                ignore = ["vendor"]
                enabled_rules = ["TRC001", "TRC008"]

                [watch]
                interval_ms = 100
            "#,
        )
        .expect("valid policy");
        assert_eq!(config.fail_on, Severity::Critical);
        assert_eq!(config.trace_window, 3);
        assert!(config.require_correlation_id);
        assert!(config.rule_enabled(Rule::UntracedError));
        assert!(!config.rule_enabled(Rule::OpaquePanic));
    }

    #[test]
    fn rejects_unknown_keys() {
        let error = Config::parse("[analysis]\nmystery = true")
            .expect_err("unknown keys must not silently weaken policy");
        assert!(error.contains("unknown key"));
    }
}
