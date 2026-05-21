// 3 種類のファイル形式 → ParsedFile 変換。
//
// 入力 raw 文字列を受け取り、構造化された ParsedFile を返す。
// malformed なら Err（DESIGN「失敗したら全体を失敗扱い」に従う）。

use crate::model::{FileKind, ParsedFile};
use std::error::Error;

pub mod gitlab_ci;
pub mod package_json;
pub mod pom_xml;

pub fn parse(kind: FileKind, raw: &str) -> Result<ParsedFile, Box<dyn Error>> {
    Ok(match kind {
        FileKind::PackageJson => ParsedFile::PackageJson(package_json::parse(raw)?),
        FileKind::PomXml => ParsedFile::PomXml(pom_xml::parse(raw)?),
        FileKind::GitlabCi => ParsedFile::GitlabCi(gitlab_ci::parse(raw)?),
    })
}
