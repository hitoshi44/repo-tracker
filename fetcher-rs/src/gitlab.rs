// GitLab API client (just the bits we need).
//
// All requests throttle to 1 fetch / sec inside send_json, per DESIGN.md.

use crate::model::{FileKind, RepoFileRef};
use reqwest::blocking::Client;
use reqwest::header;
use serde::Deserialize;
use std::error::Error;
use std::thread;
use std::time::Duration;

pub fn build_client() -> Result<Client, Box<dyn Error>> {
    // 明示的に rustls を選ぶ。reqwest がデフォルトで OS の TLS スタックを掴むのを
    // 防ぎたい (Windows の schannel + 社内プロキシで CRL/OCSP 取得が失敗するため)。
    Ok(Client::builder()
        .use_rustls_tls()
        .user_agent("repo-tracker/0.1")
        .build()?)
}

// ---------- /projects/:id ----------

#[derive(Debug, Deserialize)]
pub struct ProjectMeta {
    pub id: u64,
    pub name: String,
    pub path_with_namespace: String,
    pub web_url: String,
    pub default_branch: String,
}

pub fn fetch_project_meta(
    client: &Client,
    base_url: &str,
    project: &str,
    token: Option<&str>,
) -> Result<ProjectMeta, Box<dyn Error>> {
    let url = format!(
        "{}/api/v4/projects/{}",
        base_url.trim_end_matches('/'),
        urlencoding::encode(project),
    );
    send_json(client, &url, token)
}

// ---------- /projects/:id/repository/files/:path ----------

pub struct RawFile {
    pub blob_sha: String,
    pub size: u64,
    pub raw: String,
}

pub fn fetch_file(
    client: &Client,
    base_url: &str,
    project: &str,
    token: Option<&str>,
    ref_: &str,
    path: &str,
) -> Result<RawFile, Box<dyn Error>> {
    #[derive(Deserialize)]
    struct FileResp {
        size: u64,
        encoding: String,
        blob_id: String,
        content: String,
    }

    let url = format!(
        "{}/api/v4/projects/{}/repository/files/{}?ref={}",
        base_url.trim_end_matches('/'),
        urlencoding::encode(project),
        urlencoding::encode(path),
        urlencoding::encode(ref_),
    );
    let resp: FileResp = send_json(client, &url, token)?;

    if resp.encoding != "base64" {
        return Err(format!("unexpected encoding: {}", resp.encoding).into());
    }

    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(&resp.content)?;
    let raw = String::from_utf8(bytes)?;

    Ok(RawFile {
        blob_sha: resp.blob_id,
        size: resp.size,
        raw,
    })
}

// ---------- /projects/:id/repository/tree (walked) ----------

#[derive(Debug, Deserialize)]
struct TreeEntry {
    name: String,
    #[serde(rename = "type")]
    kind: String, // "blob" or "tree"
    path: String,
}

pub fn list_tracked_files(
    client: &Client,
    base_url: &str,
    project: &str,
    token: Option<&str>,
    nest: u32,
    targets: &[&str],
) -> Result<Vec<RepoFileRef>, Box<dyn Error>> {
    let mut results = Vec::new();
    visit_dir(
        client, base_url, project, token, "", nest, targets, &mut results,
    )?;
    Ok(results)
}

fn visit_dir(
    client: &Client,
    base_url: &str,
    project: &str,
    token: Option<&str>,
    path: &str,
    remaining: u32,
    targets: &[&str],
    out: &mut Vec<RepoFileRef>,
) -> Result<(), Box<dyn Error>> {
    let entries = fetch_tree(client, base_url, project, token, path)?;

    for e in entries {
        // println!("  {:?}", e);
        match e.kind.as_str() {
            "blob" if targets.iter().any(|t| *t == e.name) => {
                if let Some(kind) = classify(&e.name) {
                    out.push(RepoFileRef { kind, path: e.path });
                }
            }
            "tree" if remaining > 0 => {
                visit_dir(
                    client,
                    base_url,
                    project,
                    token,
                    &e.path,
                    remaining - 1,
                    targets,
                    out,
                )?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn fetch_tree(
    client: &Client,
    base_url: &str,
    project: &str,
    token: Option<&str>,
    path: &str,
) -> Result<Vec<TreeEntry>, Box<dyn Error>> {
    let path_query = if path.is_empty() {
        String::new()
    } else {
        format!("&path={}", urlencoding::encode(path))
    };
    let url = format!(
        "{}/api/v4/projects/{}/repository/tree?per_page=100{}",
        base_url.trim_end_matches('/'),
        urlencoding::encode(project),
        path_query,
    );
    send_json(client, &url, token)
}

fn classify(name: &str) -> Option<FileKind> {
    match name {
        "package.json" => Some(FileKind::PackageJson),
        "pom.xml" => Some(FileKind::PomXml),
        ".gitlab-ci.yml" => Some(FileKind::GitlabCi),
        _ => None,
    }
}

// ---------- shared GET → JSON ----------

fn send_json<T: serde::de::DeserializeOwned>(
    client: &Client,
    url: &str,
    token: Option<&str>,
) -> Result<T, Box<dyn Error>> {
    let mut req = client.get(url).header(header::ACCEPT, "application/json");
    if let Some(t) = token {
        req = req.header("PRIVATE-TOKEN", t);
    }
    let resp = req.send()?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(format!("HTTP {status}: {body}").into());
    }
    let data: T = resp.json()?;
    // Per design: throttle to 1 fetch / sec.
    thread::sleep(Duration::from_secs(1));
    Ok(data)
}
