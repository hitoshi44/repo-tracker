// .gitlab-ci.yml パーサ。
//
// 値の型ゆれが激しいので、serde で構造体に直接 deserialize はせず、
// serde_yaml_ng::Value から手で抽出する。
//
// 抽出する要素:
//   stages: Vec<String>
//   default: { image, tags }
//   variables: Map<String, String>  (値が文字列のものだけ)
//   include: Vec<CiInclude>          (string / object / 配列の混在を吸収)
//   jobs: 予約キーを除いたトップレベル object 群
//   images: 全 job + default の image を重複除いて並べた補助フィールド

use crate::model::{
    CiDefault, CiInclude, CiJob, ParsedGitlabCi,
};
use serde_yaml_ng::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;

// GitLab CI の予約キー (job 名として扱わないキー一覧)
const RESERVED_KEYS: &[&str] = &[
    "default",
    "include",
    "stages",
    "variables",
    "workflow",
    "image",
    "services",
    "cache",
    "before_script",
    "after_script",
    "pages",
    "spec",
];

pub fn parse(raw: &str) -> Result<ParsedGitlabCi, Box<dyn Error>> {
    let root: Value = serde_yaml_ng::from_str(raw)?;
    let map = match root {
        Value::Mapping(m) => m,
        Value::Null => return Ok(ParsedGitlabCi::default()),
        _ => return Err(".gitlab-ci.yml: root is not a mapping".into()),
    };

    let mut out = ParsedGitlabCi::default();

    if let Some(v) = get(&map, "stages") {
        out.stages = as_string_list(v);
    }
    if let Some(v) = get(&map, "default") {
        out.default = parse_default(v);
    }
    if let Some(v) = get(&map, "variables") {
        out.variables = as_string_map(v);
    }
    if let Some(v) = get(&map, "include") {
        out.includes = parse_includes(v);
    }

    // jobs: 予約キー以外のトップレベル object
    for (k, v) in &map {
        let Some(name) = k.as_str() else { continue };
        if RESERVED_KEYS.contains(&name) {
            continue;
        }
        if let Some(job) = parse_job(name, v) {
            out.jobs.push(job);
        }
    }

    // images: default + 全 job から重複除いた配列
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut images = Vec::new();
    if let Some(d) = &out.default {
        if let Some(img) = &d.image {
            if seen.insert(img.clone()) {
                images.push(img.clone());
            }
        }
    }
    for j in &out.jobs {
        if let Some(img) = &j.image {
            if seen.insert(img.clone()) {
                images.push(img.clone());
            }
        }
    }
    out.images = images;

    Ok(out)
}

fn get<'a>(m: &'a serde_yaml_ng::Mapping, key: &str) -> Option<&'a Value> {
    m.get(Value::String(key.to_string()))
}

// ---------- 補助: 値抽出 ----------

fn as_string(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

fn as_string_list(v: &Value) -> Vec<String> {
    match v {
        Value::String(s) => vec![s.clone()],
        Value::Sequence(xs) => xs.iter().filter_map(as_string).collect(),
        _ => vec![],
    }
}

fn as_string_map(v: &Value) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Value::Mapping(m) = v else { return out };
    for (k, vv) in m {
        let (Some(ks), Some(vs)) = (k.as_str(), value_to_simple_string(vv)) else {
            continue;
        };
        out.insert(ks.to_string(), vs);
    }
    out
}

// variables は値が string でないこともある (object 形式の expanded value, number, etc.)
// 単純文字列化できるものだけ拾う。
fn value_to_simple_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::Mapping(m) => {
            // GitLab CI の variables は `{value: ..., description: ...}` の object 形式もある
            m.get(Value::String("value".into()))
                .and_then(|x| x.as_str())
                .map(str::to_string)
        }
        _ => None,
    }
}

// ---------- default ----------

fn parse_default(v: &Value) -> Option<CiDefault> {
    let Value::Mapping(m) = v else { return None };
    let mut d = CiDefault::default();
    if let Some(img) = m.get(Value::String("image".into())) {
        d.image = extract_image(img);
    }
    if let Some(tags) = m.get(Value::String("tags".into())) {
        d.tags = as_string_list(tags);
    }
    Some(d)
}

// image は string or `{name: ..., entrypoint: [...]}`
fn extract_image(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Mapping(m) => m
            .get(Value::String("name".into()))
            .and_then(|x| x.as_str())
            .map(str::to_string),
        _ => None,
    }
}

// ---------- include ----------

fn parse_includes(v: &Value) -> Vec<CiInclude> {
    match v {
        // include: ci.yml         (単一文字列 → local)
        Value::String(s) => vec![CiInclude::Local { file: s.clone() }],
        // include: [...]
        Value::Sequence(xs) => xs.iter().filter_map(parse_single_include).collect(),
        // include: { ... }
        Value::Mapping(_) => parse_single_include(v).into_iter().collect(),
        _ => vec![],
    }
}

fn parse_single_include(v: &Value) -> Option<CiInclude> {
    match v {
        Value::String(s) => Some(CiInclude::Local { file: s.clone() }),
        Value::Mapping(m) => {
            let g = |k: &str| m.get(Value::String(k.into()));
            if let Some(s) = g("local").and_then(as_string) {
                Some(CiInclude::Local { file: s })
            } else if let Some(s) = g("remote").and_then(as_string) {
                Some(CiInclude::Remote { url: s })
            } else if let Some(s) = g("template").and_then(as_string) {
                Some(CiInclude::Template { name: s })
            } else if let Some(s) = g("component").and_then(as_string) {
                Some(CiInclude::Component { component: s })
            } else if let Some(s) = g("project").and_then(as_string) {
                // file: string or files: [strings]
                let files = if let Some(f) = g("file") {
                    as_string_list(f)
                } else if let Some(fs) = g("files") {
                    as_string_list(fs)
                } else {
                    vec![]
                };
                let ref_ = g("ref").and_then(as_string);
                Some(CiInclude::Project {
                    project: s,
                    ref_,
                    files,
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

// ---------- job ----------

fn parse_job(name: &str, v: &Value) -> Option<CiJob> {
    let m = match v {
        Value::Mapping(m) => m,
        // YAML anchor 定義などで mapping でないトップレベルキーがあれば無視
        _ => return None,
    };
    let mut j = CiJob {
        name: name.to_string(),
        stage: None,
        image: None,
        tags: vec![],
        extends: vec![],
        needs: vec![],
        script: vec![],
    };

    if let Some(s) = m.get(Value::String("stage".into())).and_then(as_string) {
        j.stage = Some(s);
    }
    if let Some(img) = m.get(Value::String("image".into())) {
        j.image = extract_image(img);
    }
    if let Some(tags) = m.get(Value::String("tags".into())) {
        j.tags = as_string_list(tags);
    }
    if let Some(ext) = m.get(Value::String("extends".into())) {
        j.extends = as_string_list(ext);
    }
    if let Some(needs) = m.get(Value::String("needs".into())) {
        j.needs = parse_needs(needs);
    }
    if let Some(script) = m.get(Value::String("script".into())) {
        j.script = flatten_script(script);
    }

    Some(j)
}

// needs: ["lint"] or [{job: "lint", artifacts: true}]
fn parse_needs(v: &Value) -> Vec<String> {
    let Value::Sequence(xs) = v else { return vec![] };
    xs.iter()
        .filter_map(|x| match x {
            Value::String(s) => Some(s.clone()),
            Value::Mapping(m) => m
                .get(Value::String("job".into()))
                .and_then(|y| y.as_str())
                .map(str::to_string),
            _ => None,
        })
        .collect()
}

// script: string | [string] | [string | [string]] (ネスト可)
fn flatten_script(v: &Value) -> Vec<String> {
    fn push(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::String(s) => out.push(s.clone()),
            Value::Sequence(xs) => {
                for x in xs {
                    push(x, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    push(v, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stages_default_variables() {
        let raw = r#"
stages: [build, test]
default:
  image: node:18
  tags: [docker, linux]
variables:
  CI_REGISTRY: registry.example.com
  TIMEOUT: 30
"#;
        let p = parse(raw).unwrap();
        assert_eq!(p.stages, vec!["build", "test"]);
        let d = p.default.unwrap();
        assert_eq!(d.image.as_deref(), Some("node:18"));
        assert_eq!(d.tags, vec!["docker", "linux"]);
        assert_eq!(p.variables.get("CI_REGISTRY").map(String::as_str), Some("registry.example.com"));
        assert_eq!(p.variables.get("TIMEOUT").map(String::as_str), Some("30"));
    }

    #[test]
    fn variables_object_form_extracts_value() {
        let raw = r#"
variables:
  DEPLOY_TARGET:
    value: staging
    description: deploy target
"#;
        let p = parse(raw).unwrap();
        assert_eq!(p.variables.get("DEPLOY_TARGET").map(String::as_str), Some("staging"));
    }

    #[test]
    fn include_single_string_is_local() {
        let p = parse("include: ci/build.yml").unwrap();
        assert_eq!(p.includes.len(), 1);
        match &p.includes[0] {
            CiInclude::Local { file } => assert_eq!(file, "ci/build.yml"),
            x => panic!("unexpected: {x:?}"),
        }
    }

    #[test]
    fn include_all_5_types() {
        let raw = r#"
include:
  - local: ci/a.yml
  - project: group/templates
    ref: main
    file: ci/java.yml
  - project: group/templates
    files: [a.yml, b.yml]
  - remote: https://example.com/ci.yml
  - template: Jobs/SAST.gitlab-ci.yml
  - component: gitlab.com/comp@1.0
"#;
        let p = parse(raw).unwrap();
        assert_eq!(p.includes.len(), 6);
        assert!(matches!(p.includes[0], CiInclude::Local { .. }));
        assert!(matches!(p.includes[1], CiInclude::Project { .. }));
        assert!(matches!(p.includes[2], CiInclude::Project { .. }));
        assert!(matches!(p.includes[3], CiInclude::Remote { .. }));
        assert!(matches!(p.includes[4], CiInclude::Template { .. }));
        assert!(matches!(p.includes[5], CiInclude::Component { .. }));

        if let CiInclude::Project { project, ref_, files } = &p.includes[1] {
            assert_eq!(project, "group/templates");
            assert_eq!(ref_.as_deref(), Some("main"));
            assert_eq!(files, &vec!["ci/java.yml".to_string()]);
        }
        if let CiInclude::Project { files, .. } = &p.includes[2] {
            assert_eq!(files, &vec!["a.yml".to_string(), "b.yml".to_string()]);
        }
    }

    #[test]
    fn jobs_excluded_from_reserved_keys() {
        let raw = r#"
stages: [build]
build:
  stage: build
  image: node:18
  script:
    - npm ci
    - npm run build
  tags: [docker]
  extends: .base
  needs: [lint]
lint:
  script: eslint .
"#;
        let p = parse(raw).unwrap();
        let names: Vec<_> = p.jobs.iter().map(|j| j.name.as_str()).collect();
        assert!(names.contains(&"build"));
        assert!(names.contains(&"lint"));
        assert!(!names.contains(&"stages"));

        let build = p.jobs.iter().find(|j| j.name == "build").unwrap();
        assert_eq!(build.stage.as_deref(), Some("build"));
        assert_eq!(build.image.as_deref(), Some("node:18"));
        assert_eq!(build.tags, vec!["docker"]);
        assert_eq!(build.extends, vec![".base"]);
        assert_eq!(build.needs, vec!["lint"]);
        assert_eq!(build.script, vec!["npm ci", "npm run build"]);

        let lint = p.jobs.iter().find(|j| j.name == "lint").unwrap();
        assert_eq!(lint.script, vec!["eslint ."]);
    }

    #[test]
    fn needs_with_object_form() {
        let raw = r#"
build:
  needs:
    - lint
    - { job: test, artifacts: true }
"#;
        let p = parse(raw).unwrap();
        assert_eq!(p.jobs[0].needs, vec!["lint", "test"]);
    }

    #[test]
    fn image_object_form() {
        let raw = r#"
build:
  image:
    name: node:18
    entrypoint: ["/bin/sh"]
"#;
        let p = parse(raw).unwrap();
        assert_eq!(p.jobs[0].image.as_deref(), Some("node:18"));
    }

    #[test]
    fn images_field_dedups_default_and_jobs() {
        let raw = r#"
default:
  image: node:18
build:
  image: node:18
test:
  image: alpine:3
"#;
        let p = parse(raw).unwrap();
        // 順序: default → 各 job (declaration 順)
        assert_eq!(p.images, vec!["node:18", "alpine:3"]);
    }

    #[test]
    fn empty_yaml_returns_default() {
        let p = parse("").unwrap();
        assert!(p.stages.is_empty());
        assert!(p.jobs.is_empty());
    }

    #[test]
    fn rejects_non_mapping_root() {
        assert!(parse("- a\n- b\n").is_err());
    }

    #[test]
    fn rejects_malformed_yaml() {
        // 未閉のフローシーケンス
        assert!(parse("stages: [build, test\n").is_err());
    }
}
