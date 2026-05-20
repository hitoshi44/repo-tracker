// Domain types for the S3 artifacts described in DESIGN.md.
//
// These represent what the fetcher *writes* (repos.json, files/*.json,
// ci-raws.json), not the GitLab API response shapes.
// Time fields are kept as String for now to avoid pulling in chrono.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------- Repository / repos.json, repos/<id>.json ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub id: u64,
    pub path_with_namespace: String,
    pub name: String,
    pub web_url: String,
    pub default_branch: String,
    pub default_branch_sha: String,
    pub last_activity_at: String,
    pub fetched_at: String,
    pub files: Vec<RepoFileRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoFileRef {
    #[serde(rename = "type")]
    pub kind: FileKind,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileKind {
    PackageJson,
    PomXml,
    GitlabCi,
}

// ---------- TrackedFile / files/<repo_id>/<path>.json ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedFile {
    pub repo_id: u64,
    pub path: String,
    #[serde(rename = "type")]
    pub kind: FileKind,
    pub blob_sha: String,
    pub size: u64,
    pub raw: String,
    pub parsed: ParsedFile,
}

// The outer `type` field is the discriminator; the inner payload is
// matched structurally via untagged.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParsedFile {
    PackageJson(ParsedPackageJson),
    PomXml(ParsedPomXml),
    GitlabCi(ParsedGitlabCi),
}

impl ParsedFile {
    pub fn empty(kind: FileKind) -> Self {
        match kind {
            FileKind::PackageJson => ParsedFile::PackageJson(Default::default()),
            FileKind::PomXml => ParsedFile::PomXml(Default::default()),
            FileKind::GitlabCi => ParsedFile::GitlabCi(Default::default()),
        }
    }
}

// ---------- parsed: package.json ----------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParsedPackageJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
    #[serde(default, rename = "devDependencies")]
    pub dev_dependencies: BTreeMap<String, String>,
    #[serde(default, rename = "peerDependencies")]
    pub peer_dependencies: BTreeMap<String, String>,
    #[serde(default)]
    pub scripts: BTreeMap<String, String>,
    #[serde(default)]
    pub engines: BTreeMap<String, String>,
}

// ---------- parsed: pom.xml ----------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParsedPomXml {
    #[serde(rename = "groupId", skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(rename = "artifactId", skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<PomCoordinate>,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
    #[serde(default)]
    pub dependencies: Vec<PomDependency>,
    #[serde(default)]
    pub plugins: Vec<PomPlugin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PomCoordinate {
    #[serde(rename = "groupId")]
    pub group_id: String,
    #[serde(rename = "artifactId")]
    pub artifact_id: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PomDependency {
    #[serde(rename = "groupId")]
    pub group_id: String,
    #[serde(rename = "artifactId")]
    pub artifact_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PomPlugin {
    #[serde(rename = "groupId")]
    pub group_id: String,
    #[serde(rename = "artifactId")]
    pub artifact_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

// ---------- parsed: .gitlab-ci.yml ----------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParsedGitlabCi {
    #[serde(default)]
    pub stages: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<CiDefault>,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
    #[serde(default)]
    pub includes: Vec<CiInclude>,
    #[serde(default)]
    pub jobs: Vec<CiJob>,
    #[serde(default)]
    pub images: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CiDefault {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum CiInclude {
    Local {
        file: String,
    },
    Project {
        project: String,
        #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
        ref_: Option<String>,
        #[serde(default)]
        files: Vec<String>,
    },
    Remote {
        url: String,
    },
    Template {
        name: String,
    },
    Component {
        component: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiJob {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub extends: Vec<String>,
    #[serde(default)]
    pub needs: Vec<String>,
    #[serde(default)]
    pub script: Vec<String>,
}

// ---------- ci-raws.json ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiRawEntry {
    pub repo_id: u64,
    pub path: String,
    pub raw: String,
}
