use crate::types::{ColorChoice, Rule, Severity};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Clone, Debug)]
pub struct Config {
    pub fail_on: Severity,
    pub trace_window: usize,
    pub production_only: bool,
    pub require_correlation_id: bool,
    pub correlation_fields: Vec<String>,
    pub ignore: Vec<String>,
    pub enabled_rules: HashSet<Rule>,
    pub watch_interval_ms: u64,
    pub daemon: DaemonConfig,
    pub notifications: NotificationConfig,
    pub ui: UiConfig,
    pub trace: TraceConfig,
}

#[derive(Clone, Debug)]
pub struct DaemonConfig {
    pub state_dir: PathBuf,
}

#[derive(Clone, Debug)]
pub struct NotificationConfig {
    pub enabled: bool,
    pub console: bool,
    pub min_severity: Severity,
    pub command: Option<String>,
    pub webhook_url: Option<String>,
    pub cooldown_seconds: u64,
}

#[derive(Clone, Debug)]
pub struct UiConfig {
    pub color: ColorChoice,
    pub emoji: bool,
    pub animate: bool,
}

#[derive(Clone, Debug)]
pub struct TraceConfig {
    pub provider: String,
    pub provider_command: Option<String>,
    pub low_model: String,
    pub medium_model: String,
    pub high_model: String,
    pub medium_threshold: u8,
    pub high_threshold: u8,
    pub branch_prefix: String,
    pub benchmark_command: String,
    pub benchmark_runs: usize,
    pub max_regression_percent: u64,
    pub max_complexity_increase: u8,
    pub max_diagnostic_increase: usize,
    pub max_context_bytes: usize,
    pub max_patch_bytes: usize,
    pub require_clean: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fail_on: Severity::Error,
            trace_window: 6,
            production_only: false,
            require_correlation_id: false,
            correlation_fields: vec![
                "correlation_id".into(),
                "request_id".into(),
                "trace_id".into(),
            ],
            ignore: vec![
                "target".into(),
                ".git".into(),
                ".kaptaind".into(),
                ".traci".into(),
            ],
            enabled_rules: Rule::ALL.into_iter().collect(),
            watch_interval_ms: 750,
            daemon: DaemonConfig {
                state_dir: PathBuf::from(".traci"),
            },
            notifications: NotificationConfig {
                enabled: true,
                console: true,
                min_severity: Severity::Error,
                command: None,
                webhook_url: None,
                cooldown_seconds: 30,
            },
            ui: UiConfig {
                color: ColorChoice::Auto,
                emoji: true,
                animate: true,
            },
            trace: TraceConfig {
                provider: "command".into(),
                provider_command: None,
                low_model: "small".into(),
                medium_model: "medium".into(),
                high_model: "large".into(),
                medium_threshold: 35,
                high_threshold: 70,
                branch_prefix: "traci/trace".into(),
                benchmark_command: "cargo test --all-targets".into(),
                benchmark_runs: 3,
                max_regression_percent: 10,
                max_complexity_increase: 0,
                max_diagnostic_increase: 0,
                max_context_bytes: 120_000,
                max_patch_bytes: 250_000,
                require_clean: true,
            },
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
                ("analysis", "production_only") => {
                    config.production_only = parse_bool(value, index + 1)?;
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
                ("daemon", "state_dir") => {
                    config.daemon.state_dir = PathBuf::from(unquote(value)?);
                }
                ("notifications", "enabled") => {
                    config.notifications.enabled = parse_bool(value, index + 1)?;
                }
                ("notifications", "console") => {
                    config.notifications.console = parse_bool(value, index + 1)?;
                }
                ("notifications", "min_severity") => {
                    config.notifications.min_severity = Severity::from_str(&unquote(value)?)?;
                }
                ("notifications", "command") => {
                    config.notifications.command = parse_optional_string(value)?;
                }
                ("notifications", "webhook_url") => {
                    config.notifications.webhook_url = parse_optional_string(value)?;
                }
                ("notifications", "cooldown_seconds") => {
                    config.notifications.cooldown_seconds = parse_number(value, index + 1)?;
                }
                ("ui", "color") => {
                    config.ui.color = ColorChoice::from_str(&unquote(value)?)?;
                }
                ("ui", "emoji") => {
                    config.ui.emoji = parse_bool(value, index + 1)?;
                }
                ("ui", "animate") => {
                    config.ui.animate = parse_bool(value, index + 1)?;
                }
                ("trace", "provider") => config.trace.provider = unquote(value)?,
                ("trace", "provider_command") => {
                    config.trace.provider_command = parse_optional_string(value)?;
                }
                ("trace", "low_model") => config.trace.low_model = unquote(value)?,
                ("trace", "medium_model") => config.trace.medium_model = unquote(value)?,
                ("trace", "high_model") => config.trace.high_model = unquote(value)?,
                ("trace", "medium_threshold") => {
                    config.trace.medium_threshold = parse_number(value, index + 1)?;
                }
                ("trace", "high_threshold") => {
                    config.trace.high_threshold = parse_number(value, index + 1)?;
                }
                ("trace", "branch_prefix") => config.trace.branch_prefix = unquote(value)?,
                ("trace", "benchmark_command") => {
                    config.trace.benchmark_command = unquote(value)?;
                }
                ("trace", "benchmark_runs") => {
                    config.trace.benchmark_runs = parse_number(value, index + 1)?;
                }
                ("trace", "max_regression_percent") => {
                    config.trace.max_regression_percent = parse_number(value, index + 1)?;
                }
                ("trace", "max_complexity_increase") => {
                    config.trace.max_complexity_increase = parse_number(value, index + 1)?;
                }
                ("trace", "max_diagnostic_increase") => {
                    config.trace.max_diagnostic_increase = parse_number(value, index + 1)?;
                }
                ("trace", "max_context_bytes") => {
                    config.trace.max_context_bytes = parse_number(value, index + 1)?;
                }
                ("trace", "max_patch_bytes") => {
                    config.trace.max_patch_bytes = parse_number(value, index + 1)?;
                }
                ("trace", "require_clean") => {
                    config.trace.require_clean = parse_bool(value, index + 1)?;
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
        if config.daemon.state_dir.as_os_str().is_empty() {
            // traci: allow - the caller records policy failures at the CLI boundary.
            return Err("daemon.state_dir cannot be empty".into());
        }
        if config.trace.medium_threshold >= config.trace.high_threshold
            || config.trace.high_threshold > 100
        {
            // traci: allow - the caller records policy failures at the CLI boundary.
            return Err("trace thresholds must satisfy medium < high <= 100".into());
        }
        if config.trace.benchmark_runs == 0 {
            // traci: allow - the caller records policy failures at the CLI boundary.
            return Err("trace.benchmark_runs must be greater than zero".into());
        }
        if config.trace.branch_prefix.trim().is_empty()
            || config.trace.benchmark_command.trim().is_empty()
        {
            // traci: allow - the caller records policy failures at the CLI boundary.
            return Err("trace branch prefix and benchmark command cannot be empty".into());
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

impl TraceConfig {
    pub fn model_for_score(&self, score: u8) -> &str {
        if score >= self.high_threshold {
            &self.high_model
        } else if score >= self.medium_threshold {
            &self.medium_model
        } else {
            &self.low_model
        }
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

fn parse_optional_string(value: &str) -> Result<Option<String>, String> {
    let parsed = unquote(value)?;
    Ok((!parsed.is_empty()).then_some(parsed))
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
                production_only = true
                require_correlation_id = true
                correlation_fields = ["request_id"]
                ignore = ["vendor"]
                enabled_rules = ["TRC001", "TRC008"]

                [watch]
                interval_ms = 100

                [daemon]
                state_dir = ".traci-test"

                [notifications]
                enabled = true
                console = false
                min_severity = "warning"
                command = ""
                webhook_url = ""
                cooldown_seconds = 5

                [ui]
                color = "never"
                emoji = false
                animate = false

                [trace]
                provider = "command"
                provider_command = "fake-provider"
                low_model = "tiny"
                medium_model = "standard"
                high_model = "large"
                medium_threshold = 30
                high_threshold = 60
                branch_prefix = "traci/test"
                benchmark_command = "cargo test"
                benchmark_runs = 2
                max_regression_percent = 5
                max_complexity_increase = 0
                max_diagnostic_increase = 0
                max_context_bytes = 10000
                max_patch_bytes = 20000
                require_clean = true
            "#,
        )
        .expect("valid policy");
        assert_eq!(config.fail_on, Severity::Critical);
        assert_eq!(config.trace_window, 3);
        assert!(config.production_only);
        assert_eq!(config.ui.color, ColorChoice::Never);
        assert!(!config.ui.emoji);
        assert!(!config.ui.animate);
        assert!(config.require_correlation_id);
        assert!(config.rule_enabled(Rule::UntracedError));
        assert!(!config.rule_enabled(Rule::OpaquePanic));
        assert_eq!(config.trace.model_for_score(29), "tiny");
        assert_eq!(config.trace.model_for_score(30), "standard");
        assert_eq!(config.trace.model_for_score(60), "large");
    }

    #[test]
    fn rejects_unknown_keys() {
        let error = Config::parse("[analysis]\nmystery = true")
            .expect_err("unknown keys must not silently weaken policy");
        assert!(error.contains("unknown key"));
    }
}
