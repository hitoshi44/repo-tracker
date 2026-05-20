#[allow(dead_code)]
mod model;
mod gitlab;

use chrono::Utc;
use model::{ParsedFile, Repository, TrackedFile};
use serde::Serialize;
use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

#[derive(Serialize)]
struct MetaFile {
    fetched_at: String,
    repo_count: usize,
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    // 公開 repo なら token なしでもアクセスできる。
    let token = env::var("GITLAB_TOKEN").ok();

    let base_url = "https://gitlab.com";
    let targets = [".gitlab-ci.yml", "package.json", "pom.xml"];

    // ハードコードの追跡対象リスト。後で config から読む。
    let repos_to_fetch: &[(&str, u32)] = &[
        ("gitlab-org/cli", 1),
        ("gitlab-org/gitlab-svgs", 1),
    ];

    let client = gitlab::build_client()?;
    let fetched_at = Utc::now().to_rfc3339();

    let mut repositories: Vec<Repository> = Vec::with_capacity(repos_to_fetch.len());
    for (project, nest) in repos_to_fetch {
        let meta = gitlab::fetch_project_meta(&client, base_url, project, token.as_deref())?;
        let sha = gitlab::fetch_branch_sha(
            &client,
            base_url,
            project,
            token.as_deref(),
            &meta.default_branch,
        )?;
        let files = gitlab::list_tracked_files(
            &client,
            base_url,
            project,
            token.as_deref(),
            *nest,
            &targets,
        )?;

        repositories.push(Repository {
            id: meta.id,
            path_with_namespace: meta.path_with_namespace,
            name: meta.name,
            web_url: meta.web_url,
            default_branch: meta.default_branch,
            default_branch_sha: sha,
            last_activity_at: meta.last_activity_at,
            fetched_at: fetched_at.clone(),
            files,
        });
    }

    let out_dir = Path::new("out");
    fs::create_dir_all(out_dir)?;

    let meta = MetaFile {
        fetched_at: fetched_at.clone(),
        repo_count: repositories.len(),
    };
    fs::write(
        out_dir.join("meta.json"),
        serde_json::to_string_pretty(&meta)?,
    )?;
    fs::write(
        out_dir.join("repos.json"),
        serde_json::to_string_pretty(&repositories)?,
    )?;

    // ファイル中身を repo ごとに取得して files/<repo_id>/<path>.json に書き出す。
    // parsed は今は空。
    let mut file_count = 0usize;
    for repo in &repositories {
        for f in &repo.files {
            let raw = gitlab::fetch_file(
                &client,
                base_url,
                &repo.path_with_namespace,
                token.as_deref(),
                &repo.default_branch,
                &f.path,
            )?;
            let tracked = TrackedFile {
                repo_id: repo.id,
                path: f.path.clone(),
                kind: f.kind,
                blob_sha: raw.blob_sha,
                size: raw.size,
                raw: raw.raw,
                parsed: ParsedFile::empty(f.kind),
            };
            let out_path = out_dir
                .join("files")
                .join(repo.id.to_string())
                .join(format!("{}.json", f.path));
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&out_path, serde_json::to_string_pretty(&tracked)?)?;
            file_count += 1;
        }
    }

    println!(
        "wrote out/meta.json, out/repos.json, and {} file(s) under out/files/",
        file_count
    );
    for r in &repositories {
        println!("  {} ({} file(s))", r.path_with_namespace, r.files.len());
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
