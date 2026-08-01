use crate::analyzer::Analysis;
use crate::config::Config;
use crate::notify::{Notification, Notifier};
use crate::report::{escape_json, render, Format};
use crate::types::Severity;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn run(paths: &[PathBuf], config: &Config, format: Format, once: bool) -> Result<(), String> {
    ensure_state_dir(&config.daemon.state_dir)?;
    if !once {
        write_pid(&config.daemon.state_dir, std::process::id())?;
    }
    let mut notifier = Notifier::new(config.notifications.clone());
    let mut previous_diagnostics = None;
    let mut previous_severity = Severity::Info;
    crate::watch::watch(
        paths,
        config,
        Duration::from_millis(config.watch_interval_ms),
        once,
        |analysis| {
            print!("{}", render_scan(analysis, format));
            let severity = highest_severity(analysis);
            let status = if analysis.diagnostics.is_empty() {
                "clean"
            } else {
                "violations"
            };
            // traci: allow - the handler emits a structured daemon status error below.
            if let Err(error) = write_status(
                &config.daemon.state_dir,
                status,
                Some(std::process::id()),
                analysis.summary.diagnostics,
                severity,
                None,
            ) {
                eprintln!("level=error operation=write_daemon_status error={error}");
            }
            if previous_diagnostics != Some(analysis.summary.diagnostics) {
                let notification = scan_notification(analysis, severity);
                for error in notifier.emit(&notification) {
                    eprintln!("level=error operation=deliver_notification error={error}");
                }
                previous_diagnostics = Some(analysis.summary.diagnostics);
            }
            previous_severity = severity;
        },
    )?;
    if once {
        write_status(
            &config.daemon.state_dir,
            "idle",
            None,
            previous_diagnostics.unwrap_or(0),
            previous_severity,
            None,
        )?;
    }
    Ok(())
}

pub fn start(
    paths: &[PathBuf],
    config: &Config,
    config_path: Option<&Path>,
    format: Format,
    interval_ms: Option<u64>,
    production_only: Option<bool>,
) -> Result<u32, String> {
    ensure_state_dir(&config.daemon.state_dir)?;
    if let Some(pid) = read_pid(&config.daemon.state_dir) {
        if process_active(pid) {
            // traci: allow - the CLI records lifecycle failures at its boundary.
            return Err(format!("Traci daemon is already running as PID {pid}"));
        }
    }
    let executable = std::env::current_exe()
        .map_err(|error| format!("cannot resolve Traci executable: {error}"))?;
    let stdout = append_log(&config.daemon.state_dir.join("daemon.out"))?;
    let stderr = append_log(&config.daemon.state_dir.join("daemon.err"))?;
    let mut command = Command::new(executable);
    command
        .args(["daemon", "run"])
        .args(paths)
        .args(["--format", format.as_str()]);
    if let Some(path) = config_path {
        command.arg("--config").arg(path);
    }
    if let Some(interval) = interval_ms {
        command.arg("--interval").arg(interval.to_string());
    }
    if let Some(production_only) = production_only {
        command.arg(if production_only {
            "--production-only"
        } else {
            "--all-code"
        });
    }
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|error| format!("cannot start Traci daemon: {error}"))?;
    write_pid(&config.daemon.state_dir, child.id())?;
    write_status(
        &config.daemon.state_dir,
        "starting",
        Some(child.id()),
        0,
        Severity::Info,
        None,
    )?;
    Ok(child.id())
}

pub fn stop(state_dir: &Path) -> Result<u32, String> {
    let pid = read_pid(state_dir).ok_or_else(|| "Traci daemon has no PID file".to_string())?;
    if !process_active(pid) {
        let _ignored = fs::remove_file(state_dir.join("daemon.pid"));
        // traci: allow - the CLI records lifecycle failures at its boundary.
        return Err(format!("Traci daemon PID {pid} is not active"));
    }
    let status = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .map_err(|error| format!("cannot stop Traci daemon PID {pid}: {error}"))?;
    if !status.success() {
        // traci: allow - the CLI records lifecycle failures at its boundary.
        return Err(format!("kill exited with {status}"));
    }
    let _ignored = fs::remove_file(state_dir.join("daemon.pid"));
    write_status(state_dir, "stopped", None, 0, Severity::Info, None)?;
    Ok(pid)
}

#[must_use]
pub fn status(state_dir: &Path, format: Format) -> String {
    let pid = read_pid(state_dir);
    let active = pid.is_some_and(process_active);
    let stored = fs::read_to_string(state_dir.join("status.json")).unwrap_or_default();
    match format {
        Format::Text | Format::Tui => format!(
            "Traci daemon: {}{}\nstate: {}\n",
            if active { "running" } else { "stopped" },
            pid.map_or_else(String::new, |value| format!(" (PID {value})")),
            state_dir.display()
        ),
        Format::Json | Format::Jsonl | Format::Sarif => {
            if stored.is_empty() {
                format!(
                    "{{\"status\":\"{}\",\"pid\":{},\"active\":{},\"diagnostics\":0,\"severity\":\"info\",\"last_scan_epoch\":0,\"last_error\":null}}",
                    if active { "running" } else { "stopped" },
                    pid.map_or_else(|| "null".into(), |value| value.to_string()),
                    active
                )
            } else {
                stored
            }
        }
    }
}

fn render_scan(analysis: &Analysis, format: Format) -> String {
    match format {
        Format::Text | Format::Tui | Format::Sarif => render(analysis, format),
        Format::Json | Format::Jsonl => {
            format!(
                "{{\"event\":\"scan\",\"timestamp\":{},\"analysis\":{}}}\n",
                epoch_seconds(),
                render(analysis, Format::Json)
            )
        }
    }
}

fn scan_notification(analysis: &Analysis, severity: Severity) -> Notification {
    let count = analysis.summary.diagnostics;
    Notification {
        event: if count == 0 { "clean" } else { "violations" }.into(),
        severity,
        title: if count == 0 {
            "Traci scan clean".into()
        } else {
            format!("Traci found {count} observability violation(s)")
        },
        body: format!(
            "{} file(s) scanned; {} error(s), {} critical",
            analysis.summary.files, analysis.summary.errors, analysis.summary.critical
        ),
        fields: vec![
            ("files".into(), analysis.summary.files.to_string()),
            ("diagnostics".into(), count.to_string()),
        ],
    }
}

fn highest_severity(analysis: &Analysis) -> Severity {
    analysis
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.severity)
        .max()
        .unwrap_or(Severity::Info)
}

fn ensure_state_dir(state_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(state_dir).map_err(|error| {
        format!(
            "cannot create daemon state {}: {error}",
            state_dir.display()
        )
    })
}

fn append_log(path: &Path) -> Result<fs::File, String> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("cannot open daemon log {}: {error}", path.display()))
}

fn write_pid(state_dir: &Path, pid: u32) -> Result<(), String> {
    fs::write(state_dir.join("daemon.pid"), pid.to_string())
        .map_err(|error| format!("cannot write daemon PID: {error}"))
}

fn read_pid(state_dir: &Path) -> Option<u32> {
    let source = fs::read_to_string(state_dir.join("daemon.pid")).ok()?; // traci: allow - missing state means stopped.
    source.trim().parse().ok() // traci: allow - malformed state means stopped.
}

fn process_active(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

fn write_status(
    state_dir: &Path,
    status: &str,
    pid: Option<u32>,
    diagnostics: usize,
    severity: Severity,
    last_error: Option<&str>,
) -> Result<(), String> {
    let json = format!(
        "{{\"status\":\"{}\",\"pid\":{},\"active\":{},\"diagnostics\":{},\"severity\":\"{}\",\"last_scan_epoch\":{},\"last_error\":{}}}",
        escape_json(status),
        pid.map_or_else(|| "null".into(), |value| value.to_string()),
        pid.is_some_and(process_active),
        diagnostics,
        severity,
        epoch_seconds(),
        last_error.map_or_else(|| "null".into(), |error| format!("\"{}\"", escape_json(error)))
    );
    let temporary = state_dir.join("status.json.tmp");
    let target = state_dir.join("status.json");
    let mut file = fs::File::create(&temporary)
        .map_err(|error| format!("cannot create daemon status: {error}"))?;
    file.write_all(json.as_bytes())
        .map_err(|error| format!("cannot write daemon status: {error}"))?;
    fs::rename(&temporary, &target)
        .map_err(|error| format!("cannot publish daemon status: {error}"))
}

fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Summary;

    #[test]
    fn machine_scan_is_single_event() {
        let analysis = Analysis {
            diagnostics: Vec::new(),
            summary: Summary {
                files: 2,
                ..Summary::default()
            },
        };
        let output = render_scan(&analysis, Format::Jsonl);
        assert!(output.contains("\"event\":\"scan\""));
        assert!(output.contains("\"files\":2"));
        assert_eq!(output.lines().count(), 1);
    }
}
