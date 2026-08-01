use crate::analyzer::{analyze_paths, collect_rust_files};
use crate::complexity::{estimate_paths, ComplexityReport};
use crate::config::Config;
use crate::report::{escape_json, Format};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TracePlan {
    pub paths: Vec<PathBuf>,
    pub goal: String,
    pub complexity_score: u8,
    pub model: String,
    pub provider: String,
    pub branch: String,
    pub apply: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Benchmark {
    pub command: String,
    pub runs_ms: Vec<u128>,
    pub median_ms: u128,
    pub success: bool,
    pub failure: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceOutcome {
    pub plan: TracePlan,
    pub merged: bool,
    pub reason: String,
    pub baseline_complexity: u8,
    pub candidate_complexity: Option<u8>,
    pub baseline_diagnostics: usize,
    pub candidate_diagnostics: Option<usize>,
    pub baseline_benchmark_ms: u128,
    pub candidate_benchmark_ms: Option<u128>,
}

pub fn plan(
    paths: &[PathBuf],
    goal: &str,
    config: &Config,
    apply: bool,
) -> Result<TracePlan, String> {
    if goal.trim().is_empty() {
        // traci: allow - the CLI records trace planning failures at its boundary.
        return Err("trace goal cannot be empty".into());
    }
    let complexity = estimate_paths(paths, config)?;
    let model = config.trace.model_for_score(complexity.score).to_string();
    Ok(TracePlan {
        paths: paths.to_vec(),
        goal: goal.trim().to_string(),
        complexity_score: complexity.score,
        model,
        provider: config.trace.provider.clone(),
        branch: format!(
            "{}/{}-{}",
            config.trace.branch_prefix.trim_end_matches('/'),
            slug(goal),
            epoch_seconds()
        ),
        apply,
    })
}

pub fn execute(plan: TracePlan, config: &Config) -> Result<TraceOutcome, String> {
    if !plan.apply {
        // traci: allow - execution requires the explicit mutation flag.
        return Err("trace execution requires --apply".into());
    }
    let repo = repository_root()?;
    if config.trace.require_clean
        && !git_output(&repo, &["status", "--porcelain"])?
            .trim()
            .is_empty()
    {
        // traci: allow - the CLI records trace safety failures at its boundary.
        return Err("trace requires a clean Git worktree".into());
    }
    let base_branch = git_output(&repo, &["branch", "--show-current"])?;
    let base_branch = base_branch.trim();
    if base_branch.is_empty() {
        // traci: allow - detached repositories cannot be merged safely.
        return Err("trace requires a named Git branch".into());
    }

    let targets = relative_targets(&repo, &plan.paths)?;
    let baseline_analysis = analyze_paths(&plan.paths, config)?;
    let baseline_complexity = estimate_paths(&plan.paths, config)?;
    let worktree = repo
        .join(&config.daemon.state_dir)
        .join("worktrees")
        .join(plan.branch.replace('/', "_"));
    if let Some(parent) = worktree.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create trace worktree parent: {error}"))?;
    }
    git_status(
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            &plan.branch,
            worktree.to_string_lossy().as_ref(),
            "HEAD",
        ],
    )?;

    let baseline_benchmark = benchmark(&worktree, config)?;
    if !baseline_benchmark.success {
        remove_worktree(&repo, &worktree, true)?;
        // traci: allow - the CLI records benchmark failures at its boundary.
        return Err(format!(
            "baseline benchmark failed: {}",
            baseline_benchmark
                .failure
                .as_deref()
                .unwrap_or("unknown failure")
        ));
    }
    let preexisting_changes = changed_paths(&worktree)?
        .into_iter()
        .collect::<HashSet<_>>();

    let worktree_paths = targets
        .iter()
        .map(|target| worktree.join(&target.path))
        .collect::<Vec<_>>();
    let mut candidate_config = config.clone();
    if let Some(state_name) = config.daemon.state_dir.file_name() {
        candidate_config
            .ignore
            .retain(|pattern| Path::new(pattern).file_name() != Some(state_name));
    }
    let context = build_context(&worktree, &worktree_paths, &candidate_config)?;
    let prompt = build_prompt(&plan, &baseline_complexity, &context);
    let patch = match run_provider(&worktree, &plan, config, &prompt) {
        Ok(patch) => patch,
        // traci: allow - the provider failure is returned with operation context.
        Err(error) => {
            remove_worktree(&repo, &worktree, true)?;
            // traci: allow - the provider failure is returned with operation context.
            return Err(error);
        }
    };
    if patch.len() > config.trace.max_patch_bytes {
        remove_worktree(&repo, &worktree, true)?;
        // traci: allow - oversized generated patches are rejected before application.
        return Err(format!(
            "provider patch is {} bytes; limit is {}",
            patch.len(),
            config.trace.max_patch_bytes
        ));
    }
    // traci: allow - rejected generated patches are discarded before evaluation.
    if let Err(error) = apply_patch(&worktree, &patch) {
        remove_worktree(&repo, &worktree, true)?;
        // traci: allow - git apply supplied detailed command context.
        return Err(error);
    }

    let changed = changed_paths(&worktree)?
        .into_iter()
        .filter(|path| !preexisting_changes.contains(path))
        .collect::<Vec<_>>();
    if changed.is_empty() {
        remove_worktree(&repo, &worktree, true)?;
        // traci: allow - a provider that emits no change cannot be benchmarked.
        return Err("provider produced no repository changes".into());
    }
    if let Some(outside) = changed.iter().find(|path| !target_allows(path, &targets)) {
        remove_worktree(&repo, &worktree, true)?;
        // traci: allow - out-of-scope generated changes are rejected and discarded.
        return Err(format!(
            "provider changed '{}' outside the requested trace paths",
            outside.display()
        ));
    }

    let candidate_analysis = analyze_paths(&worktree_paths, &candidate_config)?;
    let candidate_complexity = estimate_paths(&worktree_paths, &candidate_config)?;
    let candidate_benchmark = benchmark(&worktree, config)?;
    let verdict = evaluate(
        &baseline_analysis,
        &candidate_analysis,
        &baseline_complexity,
        &candidate_complexity,
        &baseline_benchmark,
        &candidate_benchmark,
        config,
    );

    git_add_paths(&worktree, &changed)?;
    let message = if verdict.is_ok() {
        format!("traci: {}", plan.goal)
    } else {
        format!("traci: retain rejected candidate for {}", plan.goal)
    };
    let mut commit_args = vec!["commit"];
    if verdict.is_err() {
        commit_args.push("--no-verify");
    }
    commit_args.extend(["-m", &message]);
    git_status(&worktree, &commit_args)?;
    remove_worktree(&repo, &worktree, true)?;

    let (merged, reason) = match verdict {
        Ok(reason) => {
            git_status(&repo, &["merge", "--ff-only", &plan.branch])?;
            (true, reason)
        }
        // traci: allow - rejected candidate evidence is returned to the CLI.
        Err(reason) => (false, reason),
    };
    debug_assert_eq!(
        git_output(&repo, &["branch", "--show-current"])
            .unwrap_or_default()
            .trim(),
        base_branch
    );
    Ok(TraceOutcome {
        plan,
        merged,
        reason,
        baseline_complexity: baseline_complexity.score,
        candidate_complexity: Some(candidate_complexity.score),
        baseline_diagnostics: baseline_analysis.summary.diagnostics,
        candidate_diagnostics: Some(candidate_analysis.summary.diagnostics),
        baseline_benchmark_ms: baseline_benchmark.median_ms,
        candidate_benchmark_ms: Some(candidate_benchmark.median_ms),
    })
}

#[derive(Clone, Debug)]
struct Target {
    path: PathBuf,
    directory: bool,
}

fn relative_targets(repo: &Path, paths: &[PathBuf]) -> Result<Vec<Target>, String> {
    let canonical_repo = fs::canonicalize(repo)
        .map_err(|error| format!("cannot canonicalize repository: {error}"))?;
    paths
        .iter()
        .map(|path| {
            let absolute = fs::canonicalize(path).map_err(|error| {
                format!("cannot resolve trace path {}: {error}", path.display())
            })?;
            let relative = absolute.strip_prefix(&canonical_repo).map_err(|error| {
                format!(
                    "trace path {} is outside the repository: {error}",
                    path.display()
                )
            })?;
            Ok(Target {
                path: relative.to_path_buf(),
                directory: absolute.is_dir(),
            })
        })
        .collect()
}

fn target_allows(changed: &Path, targets: &[Target]) -> bool {
    targets.iter().any(|target| {
        if target.path.as_os_str().is_empty() {
            true
        } else if target.directory {
            changed.starts_with(&target.path)
        } else {
            changed == target.path
        }
    })
}

fn build_context(root: &Path, paths: &[PathBuf], config: &Config) -> Result<String, String> {
    let files = collect_rust_files(paths, config)?;
    let mut context = String::new();
    for path in files {
        let source = fs::read_to_string(&path)
            .map_err(|error| format!("cannot read provider context {}: {error}", path.display()))?;
        let display_path = path.strip_prefix(root).unwrap_or(&path);
        let header = format!("\n--- {} ---\n", display_path.display());
        if context.len() + header.len() + source.len() > config.trace.max_context_bytes {
            break;
        }
        context.push_str(&header);
        context.push_str(&source);
    }
    Ok(context)
}

fn build_prompt(plan: &TracePlan, complexity: &ComplexityReport, context: &str) -> String {
    format!(
        "You are improving Rust code in an isolated Git worktree.\n\
         Goal: {}\nComplexity score: {}/100.\n\
         Preserve public behavior, improve maintainability and observability, and do not edit files outside the supplied context.\n\
         Return only a unified Git diff beginning with 'diff --git'. Do not use Markdown fences.\n{}",
        plan.goal, complexity.score, context
    )
}

fn run_provider(
    worktree: &Path,
    plan: &TracePlan,
    config: &Config,
    prompt: &str,
) -> Result<Vec<u8>, String> {
    let mut command = match config.trace.provider.as_str() {
        "command" => {
            let configured = config.trace.provider_command.as_deref().ok_or_else(|| {
                "trace.provider_command is required when provider = command".to_string()
            })?;
            let mut command = Command::new("sh");
            command.args(["-c", configured]);
            command
        }
        "ollama" => {
            let mut command = Command::new("ollama");
            command.args(["run", &plan.model]);
            command
        }
        other => {
            // traci: allow - unsupported providers are rejected before any mutation.
            return Err(format!("unsupported trace provider '{other}'"));
        }
    };
    let mut child = command
        .current_dir(worktree)
        .env("TRACI_MODEL", &plan.model)
        .env("TRACI_GOAL", &plan.goal)
        .env("TRACI_COMPLEXITY_SCORE", plan.complexity_score.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot start trace provider: {error}"))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(prompt.as_bytes())
            .map_err(|error| format!("cannot send prompt to trace provider: {error}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("cannot wait for trace provider: {error}"))?;
    if !output.status.success() {
        // traci: allow - provider stderr is returned with its failing status.
        return Err(format!(
            "trace provider exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    extract_patch(&output.stdout)
}

fn extract_patch(output: &[u8]) -> Result<Vec<u8>, String> {
    let text = String::from_utf8_lossy(output);
    let start = text
        .find("diff --git ")
        .ok_or_else(|| "trace provider did not return a unified Git diff".to_string())?;
    let mut patch = text[start..].trim().to_string();
    if let Some(fence) = patch.rfind("```") {
        patch.truncate(fence);
    }
    patch.push('\n');
    Ok(patch.into_bytes())
}

fn apply_patch(worktree: &Path, patch: &[u8]) -> Result<(), String> {
    let mut child = Command::new("git")
        .args(["apply", "--whitespace=fix", "-"])
        .current_dir(worktree)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot start git apply: {error}"))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(patch)
            .map_err(|error| format!("cannot write generated patch to git apply: {error}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("cannot wait for git apply: {error}"))?;
    require_success("git apply", &output)
}

fn benchmark(directory: &Path, config: &Config) -> Result<Benchmark, String> {
    let mut runs_ms = Vec::with_capacity(config.trace.benchmark_runs);
    let mut failure = None;
    for _ in 0..config.trace.benchmark_runs {
        let started = Instant::now();
        let output = Command::new("sh")
            .args(["-c", &config.trace.benchmark_command])
            .current_dir(directory)
            .output()
            .map_err(|error| format!("cannot start trace benchmark: {error}"))?;
        runs_ms.push(started.elapsed().as_millis());
        if !output.status.success() {
            failure = Some(format!(
                "{}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
            break;
        }
    }
    let mut sorted = runs_ms.clone();
    sorted.sort_unstable();
    let median_ms = sorted.get(sorted.len() / 2).copied().unwrap_or(0);
    Ok(Benchmark {
        command: config.trace.benchmark_command.clone(),
        runs_ms,
        median_ms,
        success: failure.is_none(),
        failure,
    })
}

fn evaluate(
    baseline_analysis: &crate::Analysis,
    candidate_analysis: &crate::Analysis,
    baseline_complexity: &ComplexityReport,
    candidate_complexity: &ComplexityReport,
    baseline_benchmark: &Benchmark,
    candidate_benchmark: &Benchmark,
    config: &Config,
) -> Result<String, String> {
    if !candidate_benchmark.success {
        // traci: allow - rejected candidates are retained on their branch with evidence.
        return Err(format!(
            "candidate benchmark failed: {}",
            candidate_benchmark
                .failure
                .as_deref()
                .unwrap_or("unknown failure")
        ));
    }
    let maximum_ms = baseline_benchmark
        .median_ms
        .saturating_mul(u128::from(100 + config.trace.max_regression_percent))
        / 100;
    if candidate_benchmark.median_ms > maximum_ms {
        // traci: allow - rejected candidates are retained on their branch with evidence.
        return Err(format!(
            "benchmark regressed from {} ms to {} ms (limit {} ms)",
            baseline_benchmark.median_ms, candidate_benchmark.median_ms, maximum_ms
        ));
    }
    if candidate_analysis.summary.diagnostics
        > baseline_analysis.summary.diagnostics + config.trace.max_diagnostic_increase
    {
        // traci: allow - rejected candidates are retained on their branch with evidence.
        return Err(format!(
            "diagnostics increased from {} to {}",
            baseline_analysis.summary.diagnostics, candidate_analysis.summary.diagnostics
        ));
    }
    if candidate_complexity.score
        > baseline_complexity
            .score
            .saturating_add(config.trace.max_complexity_increase)
    {
        // traci: allow - rejected candidates are retained on their branch with evidence.
        return Err(format!(
            "complexity increased from {} to {}",
            baseline_complexity.score, candidate_complexity.score
        ));
    }
    Ok(format!(
        "candidate passed: complexity {}→{}, diagnostics {}→{}, median benchmark {}→{} ms",
        baseline_complexity.score,
        candidate_complexity.score,
        baseline_analysis.summary.diagnostics,
        candidate_analysis.summary.diagnostics,
        baseline_benchmark.median_ms,
        candidate_benchmark.median_ms
    ))
}

fn changed_paths(worktree: &Path) -> Result<Vec<PathBuf>, String> {
    Ok(git_output(worktree, &["status", "--porcelain"])?
        .lines()
        .filter_map(|line| line.get(3..))
        .map(|path| path.rsplit_once(" -> ").map_or(path, |(_, target)| target))
        .map(PathBuf::from)
        .collect())
}

fn repository_root() -> Result<PathBuf, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|error| format!("cannot locate Git repository: {error}"))?;
    require_success("git rev-parse", &output)?;
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim(),
    ))
}

fn git_output(directory: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(directory)
        .output()
        .map_err(|error| format!("cannot run git {}: {error}", args.join(" ")))?;
    require_success(&format!("git {}", args.join(" ")), &output)?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn git_status(directory: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(directory)
        .output()
        .map_err(|error| format!("cannot run git {}: {error}", args.join(" ")))?;
    require_success(&format!("git {}", args.join(" ")), &output)
}

fn git_add_paths(directory: &Path, paths: &[PathBuf]) -> Result<(), String> {
    let mut command = Command::new("git");
    command
        .arg("add")
        .arg("--")
        .args(paths)
        .current_dir(directory);
    let output = command
        .output()
        .map_err(|error| format!("cannot stage trace candidate: {error}"))?;
    require_success("git add trace candidate", &output)
}

fn require_success(operation: &str, output: &Output) -> Result<(), String> {
    if output.status.success() {
        Ok(())
    } else {
        // traci: allow - command failures retain status and stderr context.
        Err(format!(
            "{operation} failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn remove_worktree(repo: &Path, worktree: &Path, force: bool) -> Result<(), String> {
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    let worktree_text = worktree.to_string_lossy();
    args.push(worktree_text.as_ref());
    git_status(repo, &args)
}

#[must_use]
pub fn render_plan(plan: &TracePlan, format: Format) -> String {
    match format {
        Format::Text | Format::Tui => format!(
            "Traci trace plan\n  goal: {}\n  complexity: {}/100\n  provider: {}\n  model: {}\n  branch: {}\n  apply: {}\n",
            plan.goal,
            plan.complexity_score,
            plan.provider,
            plan.model,
            plan.branch,
            plan.apply
        ),
        Format::Json | Format::Jsonl | Format::Sarif => format!(
            "{{\"event\":\"trace_plan\",\"goal\":\"{}\",\"complexity_score\":{},\"provider\":\"{}\",\"model\":\"{}\",\"branch\":\"{}\",\"apply\":{}}}\n",
            escape_json(&plan.goal),
            plan.complexity_score,
            escape_json(&plan.provider),
            escape_json(&plan.model),
            escape_json(&plan.branch),
            plan.apply
        ),
    }
}

#[must_use]
pub fn render_outcome(outcome: &TraceOutcome, format: Format) -> String {
    match format {
        Format::Text | Format::Tui => format!(
            "Traci trace {}\n  branch: {}\n  {}\n",
            if outcome.merged { "merged" } else { "rejected" },
            outcome.plan.branch,
            outcome.reason
        ),
        Format::Json | Format::Jsonl | Format::Sarif => format!(
            "{{\"event\":\"trace_outcome\",\"merged\":{},\"branch\":\"{}\",\"reason\":\"{}\",\"baseline\":{{\"complexity\":{},\"diagnostics\":{},\"benchmark_ms\":{}}},\"candidate\":{{\"complexity\":{},\"diagnostics\":{},\"benchmark_ms\":{}}}}}\n",
            outcome.merged,
            escape_json(&outcome.plan.branch),
            escape_json(&outcome.reason),
            outcome.baseline_complexity,
            outcome.baseline_diagnostics,
            outcome.baseline_benchmark_ms,
            option_number(outcome.candidate_complexity.map(u128::from)),
            option_number(outcome.candidate_diagnostics.map(|value| value as u128)),
            option_number(outcome.candidate_benchmark_ms)
        ),
    }
}

fn option_number(value: Option<u128>) -> String {
    value.map_or_else(|| "null".into(), |number| number.to_string())
}

fn slug(goal: &str) -> String {
    let slug = goal
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let compact = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    if compact.is_empty() {
        "improvement".into()
    } else {
        compact
    }
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
    fn model_routing_happens_in_plan_before_apply() {
        let mut config = Config::default();
        config.trace.medium_threshold = 1;
        config.trace.high_threshold = 100;
        config.trace.medium_model = "reviewer".into();
        let plan =
            plan(&[PathBuf::from("src")], "reduce branching", &config, false).expect("trace plan");
        assert_eq!(plan.model, "reviewer");
        assert!(!plan.apply);
    }

    #[test]
    fn extracts_diff_from_provider_output() {
        let output = b"analysis text\ndiff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n";
        let patch = extract_patch(output).expect("valid diff");
        assert!(patch.starts_with(b"diff --git"));
    }

    #[test]
    fn trace_json_states_mutation_intent() {
        let plan = TracePlan {
            paths: vec![PathBuf::from("src")],
            goal: "simplify".into(),
            complexity_score: 42,
            model: "standard".into(),
            provider: "command".into(),
            branch: "traci/trace/simplify-1".into(),
            apply: false,
        };
        let output = render_plan(&plan, Format::Json);
        assert!(output.contains("\"complexity_score\":42"));
        assert!(output.contains("\"apply\":false"));
    }

    #[test]
    fn evaluation_rejects_complexity_regression() {
        let analysis = crate::Analysis {
            diagnostics: Vec::new(),
            summary: Summary::default(),
        };
        let baseline = complexity_report(20);
        let candidate = complexity_report(21);
        let benchmark = Benchmark {
            command: "true".into(),
            runs_ms: vec![10],
            median_ms: 10,
            success: true,
            failure: None,
        };
        let error = evaluate(
            &analysis,
            &analysis,
            &baseline,
            &candidate,
            &benchmark,
            &benchmark,
            &Config::default(),
        )
        .expect_err("complexity increase must reject candidate");
        assert!(error.contains("complexity increased"));
    }

    fn complexity_report(score: u8) -> ComplexityReport {
        ComplexityReport {
            files: Vec::new(),
            lines: 0,
            functions: 0,
            branches: 0,
            error_paths: 0,
            async_points: 0,
            concurrency_points: 0,
            max_nesting: 0,
            score,
        }
    }
}
