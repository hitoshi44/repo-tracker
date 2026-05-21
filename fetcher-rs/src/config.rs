// fetcher 設定。
//
// 構成:
//   config.yml   GitLab URL / token env var / デフォルト nest / repos_file パス
//   repos.csv    `id,url[,nest]` 行リスト (sh で生成する想定)
//
// CSV パースと URL → path 抽出は純関数として分離してテスト対象にしている。

use serde::Deserialize;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    pub gitlab: GitlabCfg,
    #[serde(default)]
    pub defaults: Defaults,
    pub repos_file: String,
    #[serde(default = "default_output_dir")]
    pub output_dir: String,
}

fn default_output_dir() -> String { "site/data".to_string() }

/// 相対パスを config_path のディレクトリ基準で解決する汎用ヘルパ。
pub fn resolve_relative(config_path: &Path, value: &str) -> PathBuf {
    let p = Path::new(value);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    let base = config_path.parent().unwrap_or(Path::new("."));
    base.join(p)
}

#[derive(Debug, Deserialize)]
pub struct GitlabCfg {
    pub url: String,
    pub token_env: String,
}

#[derive(Debug, Deserialize)]
pub struct Defaults {
    #[serde(default = "default_nest")]
    pub nest: u32,
}
impl Default for Defaults {
    fn default() -> Self {
        Self { nest: default_nest() }
    }
}
fn default_nest() -> u32 { 1 }

/// repos.csv 1 行を表す。
/// API 呼び出しは `id` を使う (`/api/v4/projects/:id`)。
/// `url` は記録用 (fetcher 内では使わない、エラー時のヒント等)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoEntry {
    pub id: String,
    pub url: String,
    pub nest: u32,
}

pub fn load_config(path: &Path) -> Result<AppConfig, Box<dyn Error>> {
    let raw = fs::read_to_string(path)
        .map_err(|e| format!("read {}: {e}", path.display()))?;
    let cfg: AppConfig = serde_yaml_ng::from_str(&raw)
        .map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok(cfg)
}


pub fn load_repos(
    file_path: &Path,
    default_nest: u32,
) -> Result<Vec<RepoEntry>, Box<dyn Error>> {
    let raw = fs::read_to_string(file_path)
        .map_err(|e| format!("read {}: {e}", file_path.display()))?;
    parse_repos_lines(&raw, default_nest)
        .map_err(|e| format!("{}: {e}", file_path.display()).into())
}

/// CSV 形式: `id,url[,nest]`。`#` 始まりの行と空行はスキップ。
/// 列数 2 か 3 以外、または nest がパースできない場合エラー。
/// id を使って API を叩くので url は検証しない (空文字だけは弾く)。
pub fn parse_repos_lines(
    text: &str,
    default_nest: u32,
) -> Result<Vec<RepoEntry>, String> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let s = line.trim();
        if s.is_empty() || s.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = s.split(',').map(str::trim).collect();
        if parts.len() != 2 && parts.len() != 3 {
            return Err(format!(
                "line {}: expected 'id,url[,nest]' (2 or 3 fields), got {} fields",
                i + 1, parts.len()
            ));
        }
        let id = parts[0].to_string();
        let url = parts[1].to_string();
        let nest = if parts.len() == 3 {
            parts[2].parse::<u32>()
                .map_err(|e| format!("line {}: nest: {e}", i + 1))?
        } else {
            default_nest
        };
        if id.is_empty() {
            return Err(format!("line {}: empty id", i + 1));
        }
        if url.is_empty() {
            return Err(format!("line {}: empty url", i + 1));
        }
        out.push(RepoEntry { id, url, nest });
    }
    Ok(out)
}

/// `https://gitlab.com/group/repo[.git][/]` → `group/repo`。
/// gitlab_url の prefix を剥がし、末尾 `/` と `.git` を落とす。
pub fn url_to_path(url: &str, gitlab_url: &str) -> Result<String, String> {
    let url = url.trim();
    let base = gitlab_url.trim().trim_end_matches('/');
    let stripped = url.strip_prefix(base)
        .ok_or_else(|| format!("url {url:?} does not start with gitlab url {base:?}"))?;
    let cleaned = stripped
        .trim_start_matches('/')
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .trim_end_matches('/');
    if cleaned.is_empty() {
        return Err(format!("url {url:?} extracted to empty path"));
    }
    Ok(cleaned.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- url_to_path ----------
    #[test]
    fn url_to_path_basic() {
        assert_eq!(url_to_path("https://gitlab.com/group/repo", "https://gitlab.com").unwrap(), "group/repo");
    }
    #[test]
    fn url_to_path_strips_dot_git() {
        assert_eq!(url_to_path("https://gitlab.com/group/repo.git", "https://gitlab.com").unwrap(), "group/repo");
    }
    #[test]
    fn url_to_path_strips_trailing_slash() {
        assert_eq!(url_to_path("https://gitlab.com/group/repo/", "https://gitlab.com").unwrap(), "group/repo");
    }
    #[test]
    fn url_to_path_handles_base_trailing_slash() {
        assert_eq!(url_to_path("https://gitlab.com/group/repo", "https://gitlab.com/").unwrap(), "group/repo");
    }
    #[test]
    fn url_to_path_nested() {
        assert_eq!(url_to_path("https://gitlab.com/g/sub/r", "https://gitlab.com").unwrap(), "g/sub/r");
    }
    #[test]
    fn url_to_path_mismatched_base_errors() {
        assert!(url_to_path("https://other.com/g/r", "https://gitlab.com").is_err());
    }
    #[test]
    fn url_to_path_empty_path_errors() {
        assert!(url_to_path("https://gitlab.com/", "https://gitlab.com").is_err());
    }

    // ---------- parse_repos_lines ----------
    #[test]
    fn parse_repos_minimal() {
        let csv = "1,https://gitlab.com/g/a\n2,https://gitlab.com/g/b,3\n";
        let r = parse_repos_lines(csv, 1).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0], RepoEntry { id: "1".into(), url: "https://gitlab.com/g/a".into(), nest: 1 });
        assert_eq!(r[1], RepoEntry { id: "2".into(), url: "https://gitlab.com/g/b".into(), nest: 3 });
    }
    #[test]
    fn parse_repos_skips_blank_and_comments() {
        let csv = "# header comment\n\n1,https://gitlab.com/g/a\n  # indented comment too\n2,https://gitlab.com/g/b,0\n";
        let r = parse_repos_lines(csv, 1).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].nest, 0);
    }
    #[test]
    fn parse_repos_default_nest_applied() {
        let r = parse_repos_lines("1,https://gitlab.com/g/a\n", 5).unwrap();
        assert_eq!(r[0].nest, 5);
    }
    #[test]
    fn parse_repos_wrong_column_count_errors() {
        assert!(parse_repos_lines("1,url,3,extra\n", 1).is_err());
        assert!(parse_repos_lines("only_one_field\n", 1).is_err());
    }
    #[test]
    fn parse_repos_bad_nest_errors() {
        assert!(parse_repos_lines("1,https://gitlab.com/g/a,not_a_number\n", 1).is_err());
    }
    #[test]
    fn parse_repos_empty_id_errors() {
        assert!(parse_repos_lines(",https://gitlab.com/g/a\n", 1).is_err());
    }
    #[test]
    fn parse_repos_empty_url_errors() {
        assert!(parse_repos_lines("1,\n", 1).is_err());
    }
    #[test]
    fn parse_repos_trims_whitespace() {
        let r = parse_repos_lines("  1 , https://gitlab.com/g/a , 2 \n", 1).unwrap();
        assert_eq!(r[0], RepoEntry { id: "1".into(), url: "https://gitlab.com/g/a".into(), nest: 2 });
    }

    // ---------- resolve_relative ----------
    #[test]
    fn resolve_relative_to_config_dir() {
        let p = resolve_relative(Path::new("/etc/repo-tracker/config.yml"), "repos.csv");
        assert_eq!(p, PathBuf::from("/etc/repo-tracker/repos.csv"));
    }
    #[test]
    fn resolve_relative_absolute_passes_through() {
        let p = resolve_relative(Path::new("/etc/config.yml"), "/var/lib/repos.csv");
        assert_eq!(p, PathBuf::from("/var/lib/repos.csv"));
    }
}
