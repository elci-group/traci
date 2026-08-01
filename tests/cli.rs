use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should follow epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("traci-cli-{}-{nonce}", std::process::id()));
        fs::create_dir_all(root.join("src")).expect("create source fixture");
        fs::create_dir_all(root.join("tests")).expect("create test fixture");
        fs::write(root.join("src/lib.rs"), "pub fn ready() -> bool { true }\n")
            .expect("write production fixture");
        fs::write(
            root.join("tests/opaque.rs"),
            "fn opaque() { parse().unwrap(); }\n",
        )
        .expect("write non-production fixture");
        Self { root }
    }

    fn traci(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_traci"))
            .args(arguments)
            .arg(&self.root)
            .output()
            .expect("run Traci")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).expect("remove isolated CLI fixture");
    }
}

#[test]
fn production_scope_excludes_test_trees() {
    let fixture = Fixture::new();
    let production = fixture.traci(&["check", "--production-only", "--format", "json"]);
    assert!(production.status.success());
    let production_stdout = String::from_utf8(production.stdout).expect("UTF-8 production output");
    assert!(production_stdout.contains("\"files\":1"));
    assert!(production_stdout.contains("\"diagnostics\":[]"));

    let all = fixture.traci(&["check", "--all-code", "--format", "json"]);
    assert_eq!(all.status.code(), Some(2));
    let all_stdout = String::from_utf8(all.stdout).expect("UTF-8 all-code output");
    assert!(all_stdout.contains("\"files\":2"));
    assert!(all_stdout.contains("TRC007"));
}

#[test]
fn machine_output_ignores_terminal_decoration_flags() {
    let fixture = Fixture::new();
    let output = fixture.traci(&[
        "check",
        "--production-only",
        "--format",
        "json",
        "--color",
        "always",
        "--emoji",
        "--animate",
    ]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 JSON output");
    assert!(stdout.starts_with('{'));
    assert!(!stdout.contains("\x1b["));
    assert!(!stdout.contains('🔭'));
    assert!(output.stderr.is_empty());
}

#[test]
fn static_tui_renders_a_production_dashboard() {
    let fixture = Fixture::new();
    let output = fixture.traci(&[
        "tui",
        "--production-only",
        "--once",
        "--color",
        "never",
        "--no-emoji",
    ]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 TUI output");
    assert!(stdout.contains("OBSERVABILITY CONTROL"));
    assert!(stdout.contains("PRODUCTION"));
    assert!(stdout.contains('╭'));
    assert!(!stdout.contains("\x1b["));
}
