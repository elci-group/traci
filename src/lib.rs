//! Traci enforces reconstructable telemetry on Rust failure and concurrency paths.

pub mod analyzer;
pub mod config;
pub mod report;
pub mod types;
pub mod watch;

pub use analyzer::{analyze_paths, analyze_source, Analysis};
pub use config::Config;
pub use report::{render, Format};
pub use types::{Diagnostic, Rule, Severity, Summary};

pub fn fails_policy(analysis: &Analysis, threshold: Severity) -> bool {
    analysis
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity >= threshold)
}
