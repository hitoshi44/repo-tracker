// pom.xml パーサ。quick-xml の Reader で path ベースの状態マシン。
//
// 抽出する要素:
//   project.groupId / artifactId / version
//   project.parent.{groupId,artifactId,version}
//   project.properties.<任意キー>
//   project.dependencies.dependency.{groupId,artifactId,version,scope}
//   project.build.plugins.plugin.{groupId,artifactId,version}
//
// 上記以外の要素 (description, modules, profiles, build の他の子, ...) は無視。

use crate::model::{ParsedPomXml, PomCoordinate, PomDependency, PomPlugin};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::error::Error;

pub fn parse(raw: &str) -> Result<ParsedPomXml, Box<dyn Error>> {
    let mut reader = Reader::from_str(raw);
    reader.config_mut().trim_text(true);

    let mut out = ParsedPomXml::default();
    let mut stack: Vec<String> = Vec::new();
    let mut buf = Vec::new();

    // 途中構築中の要素
    let mut cur_parent: Option<PomCoordinate> = None;
    let mut cur_dep: Option<PomDependency> = None;
    let mut cur_plugin: Option<PomPlugin> = None;

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Eof => break,
            Event::Start(e) => {
                let name = local_name(&e.name().as_ref().to_vec());
                stack.push(name.clone());
                match stack.as_slice() {
                    s if s == ["project", "parent"] => {
                        cur_parent = Some(PomCoordinate {
                            group_id: String::new(),
                            artifact_id: String::new(),
                            version: String::new(),
                        });
                    }
                    s if s == ["project", "dependencies", "dependency"] => {
                        cur_dep = Some(PomDependency {
                            group_id: String::new(),
                            artifact_id: String::new(),
                            version: None,
                            scope: None,
                        });
                    }
                    s if s == ["project", "build", "plugins", "plugin"] => {
                        cur_plugin = Some(PomPlugin {
                            group_id: String::new(),
                            artifact_id: String::new(),
                            version: None,
                        });
                    }
                    _ => {}
                }
            }
            Event::End(_) => {
                let popped = stack.pop();
                // 子要素を持たない要素 (empty element) の場合、Start の直後に End が来る。
                match popped.as_deref() {
                    Some("parent") if stack.as_slice() == ["project"] => {
                        if let Some(p) = cur_parent.take() {
                            out.parent = Some(p);
                        }
                    }
                    Some("dependency") if stack.as_slice() == ["project", "dependencies"] => {
                        if let Some(d) = cur_dep.take() {
                            out.dependencies.push(d);
                        }
                    }
                    Some("plugin")
                        if stack.as_slice() == ["project", "build", "plugins"] =>
                    {
                        if let Some(p) = cur_plugin.take() {
                            out.plugins.push(p);
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(t) => {
                let text = t.unescape()?.into_owned();
                apply_text(
                    &stack,
                    &text,
                    &mut out,
                    &mut cur_parent,
                    &mut cur_dep,
                    &mut cur_plugin,
                );
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(out)
}

fn apply_text(
    stack: &[String],
    text: &str,
    out: &mut ParsedPomXml,
    cur_parent: &mut Option<PomCoordinate>,
    cur_dep: &mut Option<PomDependency>,
    cur_plugin: &mut Option<PomPlugin>,
) {
    let s = stack.iter().map(String::as_str).collect::<Vec<_>>();
    match s.as_slice() {
        ["project", "groupId"] => out.group_id = Some(text.to_string()),
        ["project", "artifactId"] => out.artifact_id = Some(text.to_string()),
        ["project", "version"] => out.version = Some(text.to_string()),

        ["project", "parent", k] => {
            if let Some(p) = cur_parent {
                match *k {
                    "groupId" => p.group_id = text.to_string(),
                    "artifactId" => p.artifact_id = text.to_string(),
                    "version" => p.version = text.to_string(),
                    _ => {}
                }
            }
        }

        ["project", "properties", key] => {
            out.properties.insert(key.to_string(), text.to_string());
        }

        ["project", "dependencies", "dependency", k] => {
            if let Some(d) = cur_dep {
                match *k {
                    "groupId" => d.group_id = text.to_string(),
                    "artifactId" => d.artifact_id = text.to_string(),
                    "version" => d.version = Some(text.to_string()),
                    "scope" => d.scope = Some(text.to_string()),
                    _ => {}
                }
            }
        }

        ["project", "build", "plugins", "plugin", k] => {
            if let Some(p) = cur_plugin {
                match *k {
                    "groupId" => p.group_id = text.to_string(),
                    "artifactId" => p.artifact_id = text.to_string(),
                    "version" => p.version = Some(text.to_string()),
                    _ => {}
                }
            }
        }

        _ => {}
    }
}

// 名前空間付き ("ns:tag") を剥がす。namespace が無ければそのまま。
fn local_name(raw: &[u8]) -> String {
    let s = std::str::from_utf8(raw).unwrap_or("");
    match s.rsplit_once(':') {
        Some((_, local)) => local.to_string(),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId>
  <artifactId>my-app</artifactId>
  <version>1.0.0</version>
  <parent>
    <groupId>org.springframework.boot</groupId>
    <artifactId>spring-boot-starter-parent</artifactId>
    <version>3.2.0</version>
  </parent>
  <properties>
    <java.version>17</java.version>
    <maven.compiler.source>17</maven.compiler.source>
  </properties>
  <dependencies>
    <dependency>
      <groupId>org.junit.jupiter</groupId>
      <artifactId>junit-jupiter</artifactId>
      <version>5.10.0</version>
      <scope>test</scope>
    </dependency>
    <dependency>
      <groupId>com.example.lib</groupId>
      <artifactId>core</artifactId>
    </dependency>
  </dependencies>
  <build>
    <plugins>
      <plugin>
        <groupId>org.apache.maven.plugins</groupId>
        <artifactId>maven-compiler-plugin</artifactId>
        <version>3.11.0</version>
      </plugin>
    </plugins>
  </build>
</project>"#;

    #[test]
    fn parses_coordinates() {
        let p = parse(SAMPLE).unwrap();
        assert_eq!(p.group_id.as_deref(), Some("com.example"));
        assert_eq!(p.artifact_id.as_deref(), Some("my-app"));
        assert_eq!(p.version.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn parses_parent() {
        let p = parse(SAMPLE).unwrap();
        let parent = p.parent.unwrap();
        assert_eq!(parent.group_id, "org.springframework.boot");
        assert_eq!(parent.artifact_id, "spring-boot-starter-parent");
        assert_eq!(parent.version, "3.2.0");
    }

    #[test]
    fn parses_properties_with_dots_in_names() {
        let p = parse(SAMPLE).unwrap();
        assert_eq!(p.properties.get("java.version").map(String::as_str), Some("17"));
        assert_eq!(p.properties.get("maven.compiler.source").map(String::as_str), Some("17"));
    }

    #[test]
    fn parses_dependencies_with_optional_fields() {
        let p = parse(SAMPLE).unwrap();
        assert_eq!(p.dependencies.len(), 2);
        let junit = &p.dependencies[0];
        assert_eq!(junit.group_id, "org.junit.jupiter");
        assert_eq!(junit.artifact_id, "junit-jupiter");
        assert_eq!(junit.version.as_deref(), Some("5.10.0"));
        assert_eq!(junit.scope.as_deref(), Some("test"));
        let core = &p.dependencies[1];
        assert_eq!(core.version, None);
        assert_eq!(core.scope, None);
    }

    #[test]
    fn parses_plugins() {
        let p = parse(SAMPLE).unwrap();
        assert_eq!(p.plugins.len(), 1);
        assert_eq!(p.plugins[0].artifact_id, "maven-compiler-plugin");
        assert_eq!(p.plugins[0].version.as_deref(), Some("3.11.0"));
    }

    #[test]
    fn ignores_unknown_top_level_elements() {
        let raw = r#"<project>
            <groupId>g</groupId>
            <artifactId>a</artifactId>
            <description>blah blah</description>
            <modules><module>m1</module></modules>
        </project>"#;
        let p = parse(raw).unwrap();
        assert_eq!(p.group_id.as_deref(), Some("g"));
        assert_eq!(p.artifact_id.as_deref(), Some("a"));
        // description / modules は無視
        assert!(p.properties.is_empty());
    }

    #[test]
    fn handles_namespace_prefix() {
        // 一部の pom は明示的 prefix を使う。local name で剥がす。
        let raw = r#"<pom:project xmlns:pom="http://maven.apache.org/POM/4.0.0">
            <pom:groupId>g</pom:groupId>
            <pom:artifactId>a</pom:artifactId>
        </pom:project>"#;
        let p = parse(raw).unwrap();
        assert_eq!(p.group_id.as_deref(), Some("g"));
        assert_eq!(p.artifact_id.as_deref(), Some("a"));
    }

    #[test]
    fn rejects_malformed_xml() {
        // ミスマッチ閉じタグ
        assert!(parse("<a>foo</b>").is_err());
    }
}
