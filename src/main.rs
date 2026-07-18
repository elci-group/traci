use std::env;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::str::FromStr;
use std::time::Duration;
use traci::{analyze_paths, fails_policy, render, Config, Format, Rule, Severity};

const DEFAULT_POLICY: &str = r#"[analysis]
fail_on = "error"
trace_window = 6
require_correlation_id = true
correlation_fields = ["correlation_id", "request_id", "trace_id"]
ignore = ["target", ".git", ".kaptaind"]
enabled_rules = ["TRC001", "TRC002", "TRC003", "TRC004", "TRC005", "TRC006", "TRC007", "TRC008"]

[watch]
interval_ms = 750
"#;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("level=error operation=traci_cli error={error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<ExitCode, String> {
    let Some(command) = args.first().map(String::as_str) else {
        print_help();
        return Ok(ExitCode::SUCCESS);
    };
    match command {
        "check" => run_check(&args[1..]),
        "watch" => run_watch(&args[1..]),
        "rules" => {
            print_rules();
            Ok(ExitCode::SUCCESS)
        }
        "explain" => run_explain(&args[1..]),
        "init" => run_init(&args[1..]),
        "help" | "-h" | "--help" => {
            print_help();
            Ok(ExitCode::SUCCESS)
        }
        "version" | "-V" | "--version" => {
            println!("traci {}", env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::SUCCESS)
        }
        // traci: allow - main records this command error with structured context.
        other => Err(format!("unknown command '{other}'; run 'traci help'")),
    }
}

#[derive(Debug)]
struct Options {
    paths: Vec<PathBuf>,
    config_path: Option<PathBuf>,
    format: Format,
    fail_on: Option<Severity>,
    interval_ms: Option<u64>,
    once: bool,
}

fn parse_options(args: &[String], watch: bool) -> Result<Options, String> {
    let mut options = Options {
        paths: Vec::new(),
        config_path: None,
        format: Format::Text,
        fail_on: None,
        interval_ms: None,
        once: false,
    };
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--config" => {
                index += 1;
                options.config_path = Some(PathBuf::from(required_value(args, index, "--config")?));
            }
            "--format" => {
                index += 1;
                options.format = Format::parse(required_value(args, index, "--format")?)?;
            }
            "--fail-on" => {
                index += 1;
                options.fail_on = Some(Severity::from_str(required_value(
                    args,
                    index,
                    "--fail-on",
                )?)?);
            }
            "--interval" if watch => {
                index += 1;
                options.interval_ms = Some(
                    required_value(args, index, "--interval")?
                        .parse()
                        .map_err(|error| format!("--interval expects milliseconds: {error}"))?,
                );
            }
            "--once" if watch => options.once = true,
            "-h" | "--help" => {
                if watch {
                    print_watch_help();
                } else {
                    print_check_help();
                }
                // traci: allow - this internal sentinel selects the successful help path.
                return Err(String::new());
            }
            option if option.starts_with('-') => {
                // traci: allow - main records this option error with structured context.
                return Err(format!("unknown option '{option}'"));
            }
            path => options.paths.push(PathBuf::from(path)),
        }
        index += 1;
    }
    if options.paths.is_empty() {
        options.paths.push(PathBuf::from("."));
    }
    Ok(options)
}

fn required_value<'a>(args: &'a [String], index: usize, option: &str) -> Result<&'a str, String> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("{option} requires a value"))
}

fn load_config(options: &Options) -> Result<Config, String> {
    let mut config = if let Some(path) = &options.config_path {
        Config::from_path(path)?
    } else {
        Config::discover(&options.paths[0])?.0
    };
    if let Some(fail_on) = options.fail_on {
        config.fail_on = fail_on;
    }
    Ok(config)
}

fn run_check(args: &[String]) -> Result<ExitCode, String> {
    let options = match parse_options(args, false) {
        Ok(options) => options,
        // traci: allow - these branches propagate to the structured CLI boundary.
        Err(error) if error.is_empty() => return Ok(ExitCode::SUCCESS),
        // traci: allow - these branches propagate to the structured CLI boundary.
        Err(error) => return Err(error),
    };
    let config = load_config(&options)?;
    let analysis = analyze_paths(&options.paths, &config)?;
    print!("{}", render(&analysis, options.format));
    Ok(if fails_policy(&analysis, config.fail_on) {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    })
}

fn run_watch(args: &[String]) -> Result<ExitCode, String> {
    let options = match parse_options(args, true) {
        Ok(options) => options,
        // traci: allow - these branches propagate to the structured CLI boundary.
        Err(error) if error.is_empty() => return Ok(ExitCode::SUCCESS),
        // traci: allow - these branches propagate to the structured CLI boundary.
        Err(error) => return Err(error),
    };
    let config = load_config(&options)?;
    let interval = options.interval_ms.unwrap_or(config.watch_interval_ms);
    if interval < 50 {
        // traci: allow - main records this option error with structured context.
        return Err("--interval must be at least 50 milliseconds".into());
    }
    eprintln!(
        "Traci is watching {} path(s) every {interval} ms; press Ctrl-C to stop",
        options.paths.len()
    );
    traci::watch::watch(
        &options.paths,
        &config,
        Duration::from_millis(interval),
        options.once,
        |analysis| {
            print!("{}", render(analysis, options.format));
            if let Err(error) = io::stdout().flush() {
                eprintln!("level=error operation=flush_watch_output error={error}");
            }
        },
    )?;
    Ok(ExitCode::SUCCESS)
}

fn run_explain(args: &[String]) -> Result<ExitCode, String> {
    let code = args
        .first()
        .ok_or_else(|| "usage: traci explain TRC001".to_string())?;
    let rule = Rule::from_code(&code.to_ascii_uppercase())
        .ok_or_else(|| format!("unknown rule '{code}'"))?;
    println!(
        "{}: {} ({})",
        rule.code(),
        rule.title(),
        rule.default_severity()
    );
    println!("{}", rule.description());
    Ok(ExitCode::SUCCESS)
}

fn run_init(args: &[String]) -> Result<ExitCode, String> {
    let force = match args {
        [] => false,
        [flag] if flag == "--force" => true,
        _ => {
            // traci: allow - main records this option error with structured context.
            return Err("usage: traci init [--force]".into());
        }
    };
    let path = Path::new("traci.toml");
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create(true)
        .truncate(force)
        .create_new(!force);
    let mut file = options.open(path).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            "traci.toml already exists; use --force to replace it".to_string()
        } else {
            format!("cannot create {}: {error}", path.display())
        }
    })?;
    file.write_all(DEFAULT_POLICY.as_bytes())
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    println!("created {}", path.display());
    Ok(ExitCode::SUCCESS)
}

fn print_rules() {
    for rule in Rule::ALL {
        println!(
            "{}  {:8}  {}",
            rule.code(),
            rule.default_severity(),
            rule.title()
        );
    }
}

fn print_help() {
    println!(
        "Traci {} — observability enforcement for Rust\n\n\
         Usage:\n  traci check [PATH...] [OPTIONS]\n  traci watch [PATH...] [OPTIONS]\n  \
         traci rules\n  traci explain RULE\n  traci init [--force]\n\n\
         Run 'traci check --help' or 'traci watch --help' for command options.",
        env!("CARGO_PKG_VERSION")
    );
}

fn print_check_help() {
    println!(
        "Usage: traci check [PATH...] [--config FILE] [--format text|json|sarif] \
         [--fail-on info|warning|error|critical]"
    );
}

fn print_watch_help() {
    println!(
        "Usage: traci watch [PATH...] [--config FILE] [--format text|json|sarif] \
         [--fail-on LEVEL] [--interval MS] [--once]"
    );
}
