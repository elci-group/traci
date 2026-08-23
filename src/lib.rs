//! Traci enforces reconstructable telemetry on Rust failure and concurrency paths.

pub mod analyzer;
pub mod complexity;
pub mod config;
pub mod daemon;
pub mod enforce;
pub mod notify;
pub mod report;
pub mod ingest;
pub mod trace;
pub mod types;
pub mod ui;
pub mod watch;

pub use analyzer::{analyze_paths, analyze_source, Analysis};
pub use complexity::{estimate_paths, estimate_source, ComplexityReport, FileComplexity};
pub use config::Config;
pub use enforce::{enforce, EnforceOptions, EnforceOutcome};
pub use report::{render, Format};
pub use types::{ColorChoice, Diagnostic, Rule, Severity, Summary};

pub fn fails_policy(analysis: &Analysis, threshold: Severity) -> bool {
    analysis
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity >= threshold)
}
