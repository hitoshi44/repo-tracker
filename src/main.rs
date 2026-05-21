#[allow(dead_code)]
mod model;
mod config;
mod gitlab;
mod parser;

use chrono::Utc;
use config::{load_config, load_repos, resolve_repos_path};
use model::{FileKind, RawEntry, Repository, TrackedFile};
use serde::Serialize;
use std::env;
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
    let repos_path = resolve_repos_path(&config_path, &cfg.repos_file);
    let entries = load_repos(&repos_path, cfg.defaults.nest, &cfg.gitlab.url)?;
    if entries.is_empty() {
        return Err(format!("no repos in {}", repos_path.display()).into());
    }

    let token = env::var(&cfg.gitlab.token_env).ok();
    let base_url = cfg.gitlab.url.trim_end_matches('/');
    let targets = [".gitlab-ci.yml", "package.json", "pom.xml"];

    let client = gitlab::build_client()?;
    let fetched_at = Utc::now().to_rfc3339();

    let out_dir = Path::new("site/data");
    fs::create_dir_all(out_dir)?;

    let mut repositories: Vec<Repository> = Vec::with_capacity(entries.len());
    let mut ci_raws: Vec<RawEntry> = Vec::new();
    let mut pkg_raws: Vec<RawEntry> = Vec::new();
    let mut pom_raws: Vec<RawEntry> = Vec::new();
    let mut file_count = 0usize;

    println!("config: {}", config_path.display());
    println!("repos:  {} ({} entries)", repos_path.display(), entries.len());

    for entry in &entries {
        let meta = gitlab::fetch_project_meta(&client, base_url, &entry.path, token.as_deref())?;
        let files = gitlab::list_tracked_files(
            &client,
            base_url,
            &entry.path,
            token.as_deref(),
            entry.nest,
            &targets,
        )?;

        // ファイル中身を取得して files/<repo_id>/<path>.json に書き出す。
        // default_branch は ref としてここでだけ使い、Repository には載せない。
        // 種別ごとに *_raws にも積んでおいて、後で *-raws.json にバンドル。
        for f in &files {
            let raw = gitlab::fetch_file(
                &client,
                base_url,
                &meta.path_with_namespace,
                token.as_deref(),
                &meta.default_branch,
                &f.path,
            )?;
            let raw_entry = RawEntry {
                repo_id: meta.id,
                path: f.path.clone(),
                raw: raw.raw.clone(),
            };
            match f.kind {
                FileKind::GitlabCi => ci_raws.push(raw_entry),
                FileKind::PackageJson => pkg_raws.push(raw_entry),
                FileKind::PomXml => pom_raws.push(raw_entry),
            }
            // DESIGN「失敗したら全体失敗」に従い、parse エラーは伝播させる。
            let parsed = parser::parse(f.kind, &raw.raw)
                .map_err(|e| format!("parse {} ({:?}): {e}", f.path, f.kind))?;
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
            file_count += 1;
        }

        repositories.push(Repository {
            id: meta.id,
            path: meta.path_with_namespace,
            name: meta.name,
            url: meta.web_url,
            fetched_at: fetched_at.clone(),
            files,
        });
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

    println!(
        "wrote site/data/repos.json, site/data/ci-raws.json ({}), site/data/pkg-raws.json ({}), site/data/pom-raws.json ({}), and {} file(s) under site/data/files/",
        ci_raws.len(),
        pkg_raws.len(),
        pom_raws.len(),
        file_count
    );
    for r in &repos_json.repos {
        println!("  {} ({} file(s))", r.path, r.files.len());
    }
    Ok(())
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
