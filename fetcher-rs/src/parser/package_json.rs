// package.json パーサ。
//
// 方針: 値の型ゆれに寛容。`dependencies`/`scripts`/`engines` などの value は
// 文字列だけ拾って、非文字列は黙って捨てる (npm の overrides 等で稀にオブジェクトが
// 混じることがあるため)。root JSON が壊れている / object でない場合のみエラー。

use crate::model::ParsedPackageJson;
use serde_json::Value;
use std::collections::BTreeMap;
use std::error::Error;

pub fn parse(raw: &str) -> Result<ParsedPackageJson, Box<dyn Error>> {
    let v: Value = serde_json::from_str(raw)?;
    let obj = v
        .as_object()
        .ok_or("package.json: root is not an object")?;

    let mut out = ParsedPackageJson::default();
    out.name = obj.get("name").and_then(|x| x.as_str()).map(str::to_string);
    out.version = obj.get("version").and_then(|x| x.as_str()).map(str::to_string);
    out.dependencies = string_map(obj.get("dependencies"));
    out.dev_dependencies = string_map(obj.get("devDependencies"));
    out.peer_dependencies = string_map(obj.get("peerDependencies"));
    out.scripts = string_map(obj.get("scripts"));
    out.engines = string_map(obj.get("engines"));
    Ok(out)
}

fn string_map(v: Option<&Value>) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    let Some(Value::Object(o)) = v else { return m };
    for (k, vv) in o {
        if let Some(s) = vv.as_str() {
            m.insert(k.clone(), s.to_string());
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typical_package_json() {
        let raw = r#"{
          "name": "my-app",
          "version": "1.2.3",
          "dependencies": { "lodash": "^4.17.21", "react": "^18.0.0" },
          "devDependencies": { "jest": "^29.0.0" },
          "scripts": { "build": "tsc", "test": "jest" },
          "engines": { "node": ">=18" }
        }"#;
        let p = parse(raw).unwrap();
        assert_eq!(p.name.as_deref(), Some("my-app"));
        assert_eq!(p.version.as_deref(), Some("1.2.3"));
        assert_eq!(p.dependencies.get("lodash").map(String::as_str), Some("^4.17.21"));
        assert_eq!(p.dev_dependencies.get("jest").map(String::as_str), Some("^29.0.0"));
        assert_eq!(p.scripts.get("build").map(String::as_str), Some("tsc"));
        assert_eq!(p.engines.get("node").map(String::as_str), Some(">=18"));
    }

    #[test]
    fn missing_fields_default_to_empty() {
        let p = parse(r#"{"name": "x"}"#).unwrap();
        assert_eq!(p.name.as_deref(), Some("x"));
        assert!(p.version.is_none());
        assert!(p.dependencies.is_empty());
        assert!(p.scripts.is_empty());
    }

    #[test]
    fn non_string_dep_values_are_dropped() {
        // npm の overrides 等で稀にあるオブジェクト値 → 拾わず無視
        let raw = r#"{
          "dependencies": {
            "lodash": "^4.0.0",
            "weird": { "version": "^1.0.0", "registry": "x" }
          }
        }"#;
        let p = parse(raw).unwrap();
        assert_eq!(p.dependencies.get("lodash").map(String::as_str), Some("^4.0.0"));
        assert!(!p.dependencies.contains_key("weird"));
    }

    #[test]
    fn rejects_non_object_root() {
        assert!(parse("[]").is_err());
        assert!(parse("\"foo\"").is_err());
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(parse("{ not json").is_err());
    }
}
