// kind 別の raw → ParsedFile 変換。
//
// Dockerfile はパースしない (raw のみ保持)。
// その他 3 種類は構造化を試みる。失敗時のハンドリングは呼び出し側 (main.rs) で
// 「warning を出して empty parsed で続行」する。

use crate::model::{FileKind, ParsedFile, ParsedUnstructured};
use std::error::Error;

pub mod gitlab_ci;
pub mod package_json;
pub mod pom_xml;

pub fn parse(kind: FileKind, raw: &str) -> Result<ParsedFile, Box<dyn Error>> {
    Ok(match kind {
        FileKind::PackageJson => ParsedFile::PackageJson(package_json::parse(raw)?),
        FileKind::PomXml => ParsedFile::PomXml(pom_xml::parse(raw)?),
        FileKind::GitlabCi => ParsedFile::GitlabCi(gitlab_ci::parse(raw)?),
        FileKind::Dockerfile => ParsedFile::Unstructured(ParsedUnstructured {}),
    })
}
