#[allow(dead_code)]
mod model;
mod config;
mod gitlab;
mod parser;

use chrono::Utc;
use config::{load_config, load_repos, resolve_relative, RepoEntry};
use model::{FileKind, ParsedFile, RawEntry, Repository, TrackedFile};
use reqwest::blocking::Client;
use serde::Serialize;
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Serialize)]
struct ReposJson {
    fetched_at: String,
    repos: Vec<Repository>,
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config_path: PathBuf = env::args().nth(1).unwrap_or_else(|| "config.yml".into()).into();
    let cfg = load_config(&config_path)?;
    let repos_path = resolve_relative(&config_path, &cfg.repos_file);
    let entries = load_repos(&repos_path, cfg.defaults.nest)?;
    if entries.is_empty() {
        return Err(format!("no repos in {}", repos_path.display()).into());
    }

    let token = env::var(&cfg.gitlab.token_env).ok();
    let base_url = cfg.gitlab.url.trim_end_matches('/');
    let targets = [".gitlab-ci.yml", "package.json", "pom.xml", "Dockerfile"];

    let client = gitlab::build_client()?;
    let fetched_at = Utc::now().to_rfc3339();

    let out_dir = resolve_relative(&config_path, &cfg.output_dir);
    fs::create_dir_all(&out_dir)?;

    let mut repositories: Vec<Repository> = Vec::with_capacity(entries.len());
    let mut ci_raws: Vec<RawEntry> = Vec::new();
    let mut pkg_raws: Vec<RawEntry> = Vec::new();
    let mut pom_raws: Vec<RawEntry> = Vec::new();
    let mut docker_raws: Vec<RawEntry> = Vec::new();
    let mut file_count = 0usize;

    println!("config: {}", config_path.display());
    println!("repos:  {} ({} entries)", repos_path.display(), entries.len());
    println!("output: {}", out_dir.display());

    let mut skipped = 0usize;
    for entry in &entries {
        match process_one_repo(
            &client, base_url, token.as_deref(), entry, &targets, &out_dir, &fetched_at,
        ) {
            Ok(result) => {
                ci_raws.extend(result.ci_raws);
                pkg_raws.extend(result.pkg_raws);
                pom_raws.extend(result.pom_raws);
                docker_raws.extend(result.docker_raws);
                file_count += result.files_written;
                repositories.push(result.repo);
            }
            Err(e) => {
                eprintln!("warning: skip id={} ({}): {e}", entry.id, entry.url);
                skipped += 1;
            }
        }
    }

    let repos_json = ReposJson {
        fetched_at: fetched_at.clone(),
        repos: repositories,
    };
    fs::write(
        out_dir.join("repos.json"),
        serde_json::to_string_pretty(&repos_json)?,
    )?;
    fs::write(
        out_dir.join("ci-raws.json"),
        serde_json::to_string_pretty(&ci_raws)?,
    )?;
    fs::write(
        out_dir.join("pkg-raws.json"),
        serde_json::to_string_pretty(&pkg_raws)?,
    )?;
    fs::write(
        out_dir.join("pom-raws.json"),
        serde_json::to_string_pretty(&pom_raws)?,
    )?;
    fs::write(
        out_dir.join("docker-raws.json"),
        serde_json::to_string_pretty(&docker_raws)?,
    )?;

    println!(
        "wrote {}: repos.json ({} ok, {} skipped), ci-raws.json ({}), pkg-raws.json ({}), pom-raws.json ({}), docker-raws.json ({}), and {} file(s) under files/",
        out_dir.display(),
        repos_json.repos.len(),
        skipped,
        ci_raws.len(),
        pkg_raws.len(),
        pom_raws.len(),
        docker_raws.len(),
        file_count
    );
    for r in &repos_json.repos {
        println!("  {} ({} file(s))", r.path, r.files.len());
    }
    Ok(())
}

struct RepoResult {
    repo: Repository,
    ci_raws: Vec<RawEntry>,
    pkg_raws: Vec<RawEntry>,
    pom_raws: Vec<RawEntry>,
    docker_raws: Vec<RawEntry>,
    files_written: usize,
}

/// 1 repo 分の処理。HTTP エラーが起きたらそのまま Err を返す (呼び出し側でスキップ)。
/// parse 失敗はファイル単位で吸収 (warning + raw のみ)。
///
/// ファイルは即時書き出し。失敗時に site/data/files/<id>/ にゴミが残るが、
/// repos.json に該当 id が載らないのでフロントからは参照されない。
fn process_one_repo(
    client: &Client,
    base_url: &str,
    token: Option<&str>,
    entry: &RepoEntry,
    targets: &[&str],
    out_dir: &Path,
    fetched_at: &str,
) -> Result<RepoResult, Box<dyn Error>> {
    // GitLab API は id でも path でも :id を受けるので、入力の id を一貫して使う。
    let meta = gitlab::fetch_project_meta(client, base_url, &entry.id, token)
        .map_err(|e| format!("meta: {e}"))?;
    let files = gitlab::list_tracked_files(client, base_url, &entry.id, token, entry.nest, targets)
        .map_err(|e| format!("tree: {e}"))?;

    let mut out = RepoResult {
        repo: Repository {
            id: meta.id,
            path: meta.path_with_namespace.clone(),
            name: meta.name.clone(),
            url: meta.web_url.clone(),
            fetched_at: fetched_at.to_string(),
            files: files.clone(),
        },
        ci_raws: Vec::new(),
        pkg_raws: Vec::new(),
        pom_raws: Vec::new(),
        docker_raws: Vec::new(),
        files_written: 0,
    };

    for f in &files {
        let raw = gitlab::fetch_file(client, base_url, &entry.id, token, &meta.default_branch, &f.path)
            .map_err(|e| format!("file {}: {e}", f.path))?;
        let raw_entry = RawEntry {
            repo_id: meta.id,
            path: f.path.clone(),
            raw: raw.raw.clone(),
        };
        match f.kind {
            FileKind::GitlabCi => out.ci_raws.push(raw_entry),
            FileKind::PackageJson => out.pkg_raws.push(raw_entry),
            FileKind::PomXml => out.pom_raws.push(raw_entry),
            FileKind::Dockerfile => out.docker_raws.push(raw_entry),
        }
        // parse 失敗はここで吸収 (raw のみ保存して続行)。
        let parsed = match parser::parse(f.kind, &raw.raw) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("warning: parse {} ({:?}) failed: {e} — raw のみで続行", f.path, f.kind);
                ParsedFile::empty(f.kind)
            }
        };
        let tracked = TrackedFile {
            repo_id: meta.id,
            path: f.path.clone(),
            kind: f.kind,
            blob_sha: raw.blob_sha,
            size: raw.size,
            raw: raw.raw,
            parsed,
        };
        let out_path = out_dir
            .join("files")
            .join(meta.id.to_string())
            .join(format!("{}.json", f.path));
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&out_path, serde_json::to_string_pretty(&tracked)?)?;
        out.files_written += 1;
    }
    Ok(out)
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
