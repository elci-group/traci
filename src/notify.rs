use crate::config::NotificationConfig;
use crate::report::escape_json;
use crate::types::Severity;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Notification {
    pub event: String,
    pub severity: Severity,
    pub title: String,
    pub body: String,
    pub fields: Vec<(String, String)>,
}

impl Notification {
    #[must_use]
    pub fn json(&self) -> String {
        let fields = self
            .fields
            .iter()
            .map(|(key, value)| format!("\"{}\":\"{}\"", escape_json(key), escape_json(value)))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"event\":\"{}\",\"severity\":\"{}\",\"title\":\"{}\",\"body\":\"{}\",\"fields\":{{{fields}}}}}",
            escape_json(&self.event),
            self.severity,
            escape_json(&self.title),
            escape_json(&self.body)
        )
    }
}

pub struct Notifier {
    config: NotificationConfig,
    last_sent: Option<Instant>,
}

impl Notifier {
    #[must_use]
    pub fn new(config: NotificationConfig) -> Self {
        Self {
            config,
            last_sent: None,
        }
    }

    pub fn emit(&mut self, notification: &Notification) -> Vec<String> {
        if !self.config.enabled || notification.severity < self.config.min_severity {
            return Vec::new();
        }
        if self
            .last_sent
            .is_some_and(|last| last.elapsed() < Duration::from_secs(self.config.cooldown_seconds))
        {
            return Vec::new();
        }
        self.last_sent = Some(Instant::now());
        let mut errors = Vec::new();
        if self.config.console {
            eprintln!(
                "Traci notification [{}] {} — {}",
                notification.severity, notification.title, notification.body
            );
        }
        if let Some(command) = &self.config.command {
            if let Err(error) = run_command(command, notification) {
                errors.push(error);
            }
        }
        if let Some(url) = &self.config.webhook_url {
            if let Err(error) = send_webhook(url, notification) {
                errors.push(error);
            }
        }
        errors
    }
}

fn run_command(command: &str, notification: &Notification) -> Result<(), String> {
    let mut child = Command::new("sh")
        .args(["-c", command])
        .env("TRACI_EVENT", &notification.event)
        .env("TRACI_SEVERITY", notification.severity.as_str())
        .env("TRACI_TITLE", &notification.title)
        .env("TRACI_BODY", &notification.body)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| format!("notification command failed to start: {error}"))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(notification.json().as_bytes())
            .map_err(|error| format!("notification command input failed: {error}"))?;
    }
    let status = child
        .wait()
        .map_err(|error| format!("notification command wait failed: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        // traci: allow - the daemon records notification delivery failures.
        Err(format!("notification command exited with {status}"))
    }
}

fn send_webhook(url: &str, notification: &Notification) -> Result<(), String> {
    let mut child = Command::new("curl")
        .args([
            "--fail-with-body",
            "--silent",
            "--show-error",
            "--request",
            "POST",
            "--header",
            "content-type: application/json",
            "--data-binary",
            "@-",
            url,
        ])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| format!("webhook client failed to start: {error}"))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(notification.json().as_bytes())
            .map_err(|error| format!("webhook input failed: {error}"))?;
    }
    let status = child
        .wait()
        .map_err(|error| format!("webhook wait failed: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        // traci: allow - the daemon records notification delivery failures.
        Err(format!("webhook client exited with {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_json_is_machine_readable() {
        let notification = Notification {
            event: "scan".into(),
            severity: Severity::Error,
            title: "Failure \"path\"".into(),
            body: "two findings".into(),
            fields: vec![("count".into(), "2".into())],
        };
        let json = notification.json();
        assert!(json.contains("\"event\":\"scan\""));
        assert!(json.contains("Failure \\\"path\\\""));
        assert!(json.contains("\"count\":\"2\""));
    }

    #[test]
    fn severity_threshold_suppresses_low_priority_events() {
        let mut notifier = Notifier::new(NotificationConfig {
            enabled: true,
            console: false,
            min_severity: Severity::Critical,
            command: None,
            webhook_url: None,
            cooldown_seconds: 0,
        });
        let errors = notifier.emit(&Notification {
            event: "scan".into(),
            severity: Severity::Warning,
            title: "warning".into(),
            body: "ignored".into(),
            fields: Vec::new(),
        });
        assert!(errors.is_empty());
        assert!(notifier.last_sent.is_none());
    }
}
