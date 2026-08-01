use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

impl ColorChoice {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

impl FromStr for ColorChoice {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "always" => Ok(Self::Always),
            "never" => Ok(Self::Never),
            // traci: allow - main records this option error with structured context.
            other => Err(format!("unknown color mode '{other}'")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Severity {
    Info,
    Warning,
    Error,
    Critical,
}

impl Severity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Critical => "critical",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for Severity {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "info" => Ok(Self::Info),
            "warning" | "warn" => Ok(Self::Warning),
            "error" => Ok(Self::Error),
            "critical" => Ok(Self::Critical),
            // traci: allow - main records this option error with structured context.
            other => Err(format!("unknown severity '{other}'")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Rule {
    UntracedError,
    SwallowedResult,
    GenericMessage,
    UnstructuredEvent,
    DetachedAsyncTask,
    DiscardedErrorContext,
    OpaquePanic,
    MissingCorrelation,
}

impl Rule {
    pub const ALL: [Self; 8] = [
        Self::UntracedError,
        Self::SwallowedResult,
        Self::GenericMessage,
        Self::UnstructuredEvent,
        Self::DetachedAsyncTask,
        Self::DiscardedErrorContext,
        Self::OpaquePanic,
        Self::MissingCorrelation,
    ];

    pub const fn code(self) -> &'static str {
        match self {
            Self::UntracedError => "TRC001",
            Self::SwallowedResult => "TRC002",
            Self::GenericMessage => "TRC003",
            Self::UnstructuredEvent => "TRC004",
            Self::DetachedAsyncTask => "TRC005",
            Self::DiscardedErrorContext => "TRC006",
            Self::OpaquePanic => "TRC007",
            Self::MissingCorrelation => "TRC008",
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::UntracedError => "untraced error path",
            Self::SwallowedResult => "swallowed result",
            Self::GenericMessage => "generic diagnostic message",
            Self::UnstructuredEvent => "unstructured trace event",
            Self::DetachedAsyncTask => "detached async trace context",
            Self::DiscardedErrorContext => "discarded error context",
            Self::OpaquePanic => "opaque panic path",
            Self::MissingCorrelation => "missing correlation context",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::UntracedError => "Emit a structured event before returning or handling an error.",
            Self::SwallowedResult => "Handle the result or record why discarding it is safe.",
            Self::GenericMessage => "Name the operation, failure, and distinguishing context.",
            Self::UnstructuredEvent => "Attach structured key-value fields to the event.",
            Self::DetachedAsyncTask => "Instrument spawned work or propagate the current span.",
            Self::DiscardedErrorContext => "Preserve the source error when mapping it.",
            Self::OpaquePanic => "Replace unwrap/expect with a traced, contextual error path.",
            Self::MissingCorrelation => "Accept or attach a configured correlation field.",
        }
    }

    pub const fn default_severity(self) -> Severity {
        match self {
            Self::GenericMessage
            | Self::UnstructuredEvent
            | Self::DiscardedErrorContext
            | Self::MissingCorrelation => Severity::Warning,
            Self::UntracedError | Self::SwallowedResult | Self::DetachedAsyncTask => {
                Severity::Error
            }
            Self::OpaquePanic => Severity::Critical,
        }
    }

    pub fn from_code(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|rule| rule.code() == value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub rule: Rule,
    pub severity: Severity,
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub snippet: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Summary {
    pub files: usize,
    pub diagnostics: usize,
    pub info: usize,
    pub warnings: usize,
    pub errors: usize,
    pub critical: usize,
}

impl Summary {
    pub fn from_diagnostics(files: usize, diagnostics: &[Diagnostic]) -> Self {
        let mut summary = Self {
            files,
            diagnostics: diagnostics.len(),
            ..Self::default()
        };
        for diagnostic in diagnostics {
            match diagnostic.severity {
                Severity::Info => summary.info += 1,
                Severity::Warning => summary.warnings += 1,
                Severity::Error => summary.errors += 1,
                Severity::Critical => summary.critical += 1,
            }
        }
        summary
    }
}
