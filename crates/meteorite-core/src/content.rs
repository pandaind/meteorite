//! Reading content files: front matter + Markdown body.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::error::{io_err, Error, Result};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FrontMatter {
    pub title: String,
    pub date: Option<String>,
    pub expiry_date: Option<String>,
    pub draft: bool,
    pub slug: Option<String>,
    pub template: Option<String>,
    pub description: Option<String>,
    pub weight: i64,
    pub tags: Vec<String>,
    /// Section pages only: `date` (newest first, default), `weight`, or `title`.
    pub sort_by: Option<String>,
    /// Section pages only: page size for the listing.
    pub paginate_by: Option<usize>,
    /// Any other keys are kept and exposed to templates as `page.meta.<key>`.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_norway::Value>,
}

/// A content file after front matter parsing, before URL assignment or rendering.
#[derive(Debug, Clone)]
pub struct RawPage {
    pub meta: FrontMatter,
    /// Absolute (or root-relative) path of the file.
    pub file: PathBuf,
    /// Path relative to `content/`.
    pub source: PathBuf,
    pub body: String,
}

struct Split<'a> {
    yaml: Option<(&'a str, usize)>,
    body: &'a str,
}

fn split_front_matter<'a>(raw: &'a str, path: &Path) -> Result<Split<'a>> {
    let bom = if raw.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };
    let text = &raw[bom..];
    let open = if text.starts_with("---\n") {
        4
    } else if text.starts_with("---\r\n") {
        5
    } else {
        return Ok(Split {
            yaml: None,
            body: text,
        });
    };
    let rest = &text[open..];
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            return Ok(Split {
                yaml: Some((&rest[..offset], bom + open)),
                body: &rest[offset + line.len()..],
            });
        }
        offset += line.len();
    }
    Err(Error::UnclosedFrontMatter {
        src: miette::NamedSource::new(path.display().to_string(), raw.to_string()),
        span: (bom, 3).into(),
    })
}

pub fn parse_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d);
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.date_naive());
    }
    for fmt in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Some(dt.date());
        }
    }
    None
}

pub fn load_raw(content_dir: &Path, file: &Path) -> Result<RawPage> {
    let raw = std::fs::read_to_string(file).map_err(io_err(file))?;
    let split = split_front_matter(&raw, file)?;
    let meta: FrontMatter = match split.yaml {
        Some((yaml, offset)) if !yaml.trim().is_empty() => {
            serde_norway::from_str(yaml).map_err(|e| Error::yaml(file, &raw, offset, &e))?
        }
        _ => FrontMatter::default(),
    };
    let source = file.strip_prefix(content_dir).unwrap_or(file).to_path_buf();
    Ok(RawPage {
        meta,
        file: file.to_path_buf(),
        source,
        body: split.body.to_string(),
    })
}

/// Load every `.md` file under `content_dir`, sorted by path for deterministic output.
pub fn load_all(content_dir: &Path) -> Result<Vec<RawPage>> {
    if !content_dir.is_dir() {
        return Err(Error::Msg(format!(
            "{} does not exist",
            content_dir.display()
        )));
    }
    let mut pages = Vec::new();
    for entry in WalkDir::new(content_dir).sort_by_file_name() {
        let entry = entry.map_err(|e| Error::Io {
            path: content_dir.to_path_buf(),
            source: e.into(),
        })?;
        let path = entry.path();
        if entry.file_type().is_file() && path.extension().is_some_and(|e| e == "md") {
            pages.push(load_raw(content_dir, path)?);
        }
    }
    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_matter_split() {
        let p = Path::new("x.md");
        let s = split_front_matter("---\ntitle: A\n---\nhi\n", p).unwrap();
        assert_eq!(s.yaml, Some(("title: A\n", 4)));
        assert_eq!(s.body, "hi\n");
        assert!(split_front_matter("---\ntitle: A\n", p).is_err());
        assert!(split_front_matter("no fm", p).unwrap().yaml.is_none());
    }

    #[test]
    fn dates() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 10).unwrap();
        assert_eq!(parse_date("2026-10-10"), Some(d));
        assert_eq!(parse_date("2026-10-10T08:00:00Z"), Some(d));
        assert_eq!(parse_date("2026-10-10 08:00:00"), Some(d));
        assert_eq!(parse_date("tomorrow"), None);
    }
}
