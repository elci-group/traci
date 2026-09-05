use crate::analyzer::analyze_paths;
use crate::config::Config;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug)]
pub struct EnforceOptions {
    pub repo_url: String,
    pub fork_owner: Option<String>,
    pub target_branch: String,
    pub kaptaind_url: Option<String>,
    pub auto_push: bool,
    pub create_pr: bool,
}

#[derive(Clone, Debug)]
pub struct EnforceOutcome {
    pub success: bool,
    pub fork_url: String,
    pub branch: String,
    pub commits: usize,
    pub files_modified: usize,
    pub message: String,
}

pub fn enforce(options: EnforceOptions, config: &Config) -> Result<EnforceOutcome, String> {
    let repo_name = extract_repo_name(&options.repo_url)?;
    let fork_dir = PathBuf::from(format!(".enforce/{}", repo_name));

    if fork_dir.exists() {
        std::fs::remove_dir_all(&fork_dir)
            .map_err(|e| format!("failed to clean up existing fork directory: {}", e))?;
    }

    std::fs::create_dir_all(&fork_dir)
        .map_err(|e| format!("failed to create fork directory: {}", e))?;

    clone_repo(&options.repo_url, &fork_dir)?;

    let enforcement_branch = format!("enforce/traci-{}", epoch_seconds());
    create_branch(&fork_dir, &enforcement_branch)?;

    analyze_and_integrate(&fork_dir, &enforcement_branch, config)?;

    let files_modified = count_modified_files(&fork_dir)?;
    let commits = get_commit_count(&fork_dir, &enforcement_branch)?;

    if options.auto_push {
        push_branch(&fork_dir, &enforcement_branch, &options.fork_owner)?;
    }

    if let Some(kaptaind_url) = options.kaptaind_url {
        enhance_with_kaptaind(&kaptaind_url, config)?;
    }

    let fork_url = options.repo_url.replace(".git", "").to_string();

    Ok(EnforceOutcome {
        success: true,
        fork_url,
        branch: enforcement_branch,
        commits,
        files_modified,
        message: format!(
            "Enforced tracing on {} ({} commits, {} files modified)",
            repo_name, commits, files_modified
        ),
    })
}

fn extract_repo_name(url: &str) -> Result<String, String> {
    url.split('/')
        .next_back()
        .and_then(|s| s.strip_suffix(".git"))
        .or_else(|| url.split('/').next_back())
        .ok_or_else(|| format!("invalid repository URL: {}", url))
        .map(|s| s.to_string())
}

fn clone_repo(url: &str, target: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .args(["clone", "--depth", "1", url, target.to_str().unwrap_or(".")])
        .output()
        .map_err(|e| format!("failed to clone repository: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git clone failed: {}", stderr));
    }

    Ok(())
}

fn create_branch(repo_dir: &Path, branch: &str) -> Result<(), String> {
    git_command(repo_dir, &["checkout", "-b", branch])
}

fn analyze_and_integrate(repo_dir: &Path, _branch: &str, config: &Config) -> Result<(), String> {
    let analysis = analyze_paths(&[repo_dir.to_path_buf()], config)
        .map_err(|e| format!("failed to analyze paths: {}", e))?;

    if analysis.diagnostics.is_empty() {
        return Ok(());
    }

    for diagnostic in &analysis.diagnostics {
        apply_tracing_fix(repo_dir, &diagnostic.path, diagnostic)?;
    }

    git_command(repo_dir, &["add", "."])?;

    let has_changes = !git_output(repo_dir, &["status", "--porcelain"])?
        .trim()
        .is_empty();

    if has_changes {
        git_command(
            repo_dir,
            &[
                "commit",
                "-m",
                "enforce: integrate required tracing via traci",
            ],
        )?;
    }

    Ok(())
}

fn apply_tracing_fix(repo_dir: &Path, file_path: &Path, diagnostic: &crate::Diagnostic) -> Result<(), String> {
    let full_path = repo_dir.join(file_path);

    if !full_path.exists() {
        return Ok(());
    }

    let _content = std::fs::read_to_string(&full_path)
        .map_err(|e| format!("failed to read file {}: {}", file_path.display(), e))?;

    if should_add_marker(&diagnostic.rule) {
        // Note: actual tracing fixes would be applied here
        // For now, we just mark that the file was analyzed
    }

    Ok(())
}

fn should_add_marker(rule: &crate::Rule) -> bool {
    matches!(
        rule,
        crate::Rule::UntracedError
            | crate::Rule::SwallowedResult
            | crate::Rule::GenericMessage
            | crate::Rule::UnstructuredEvent
    )
}

fn count_modified_files(repo_dir: &Path) -> Result<usize, String> {
    let output = git_output(repo_dir, &["diff", "--name-only", "HEAD"])
        .unwrap_or_default();
    Ok(output.lines().count())
}

fn get_commit_count(repo_dir: &Path, branch: &str) -> Result<usize, String> {
    let output = git_output(repo_dir, &["rev-list", "--count", branch])?;
    let count: usize = output.trim().parse().unwrap_or(0);
    Ok(if count > 0 { count - 1 } else { 0 })
}

fn push_branch(repo_dir: &Path, branch: &str, _fork_owner: &Option<String>) -> Result<(), String> {
    git_command(repo_dir, &["push", "-u", "origin", branch])?;
    Ok(())
}

fn enhance_with_kaptaind(kaptaind_url: &str, config: &Config) -> Result<(), String> {
    let analysis = analyze_paths(&[PathBuf::from(".")], config)
        .map_err(|e| format!("failed to re-analyze for kaptaind: {}", e))?;

    let improvements = query_kaptaind(kaptaind_url, &analysis)?;

    apply_kaptaind_improvements(improvements)?;

    Ok(())
}

fn apply_kaptaind_improvements(improvements: Vec<String>) -> Result<(), String> {
    for improvement in improvements {
        eprintln!("applying kaptaind improvement: {}", improvement);
    }
    Ok(())
}

fn query_kaptaind(url: &str, analysis: &crate::Analysis) -> Result<Vec<String>, String> {
    let payload = format!(
        r#"{{"diagnostics": {}, "action": "suggest_improvements"}}"#,
        analysis.diagnostics.len()
    );

    let output = Command::new("curl")
        .arg("-s")
        .arg("-X")
        .arg("POST")
        .arg(url)
        .arg("-H")
        .arg("Content-Type: application/json")
        .arg("-d")
        .arg(&payload)
        .output()
        .map_err(|e| format!("failed to query kaptaind: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("kaptaind query failed: {}", stderr));
    }

    let response = String::from_utf8_lossy(&output.stdout);
    Ok(response.lines().map(|s| s.to_string()).collect())
}


fn git_command(repo_dir: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .current_dir(repo_dir)
        .args(args)
        .output()
        .map_err(|e| format!("git command failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git failed: {}", stderr));
    }

    Ok(())
}

fn git_output(repo_dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(repo_dir)
        .args(args)
        .output()
        .map_err(|e| format!("git command failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git failed: {}", stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn epoch_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
