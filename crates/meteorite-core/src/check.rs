//! `meteorite check`: build into a scratch directory and verify internal links.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::build::{BuildOptions, BuildReport, Builder};
use crate::error::{io_err, Result};

const CHECK_ORIGIN: &str = "http://meteorite-check.invalid";

#[derive(Debug)]
pub struct Problem {
    /// Output file (relative to the build) containing the link.
    pub page: PathBuf,
    pub link: String,
    pub reason: String,
}

#[derive(Debug)]
pub struct CheckReport {
    pub build: BuildReport,
    pub files_checked: usize,
    pub links_checked: usize,
    pub problems: Vec<Problem>,
}

pub fn run(root: &Path, opts: &BuildOptions) -> Result<CheckReport> {
    let scratch = std::env::temp_dir().join(format!(
        "meteorite-check-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let mut o = opts.clone();
    o.out_dir = Some(scratch.clone());
    o.skip_knowledge_export = true;
    // Build against a placeholder absolute origin so feeds and the sitemap are generated
    // too; links under it are treated as internal when analysing.
    o.base_url = Some(format!("{CHECK_ORIGIN}/"));
    let result = (|| {
        let build = Builder::new(root).build(&o)?;
        analyse(&scratch, build)
    })();
    let _ = fs::remove_dir_all(&scratch);
    result
}

fn analyse(out: &Path, build: BuildReport) -> Result<CheckReport> {
    let mut problems = Vec::new();
    let mut files = 0;
    let mut links = 0;
    let mut exists_cache: HashSet<PathBuf> = HashSet::new();

    for entry in WalkDir::new(out).sort_by_file_name() {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if !entry.file_type().is_file() || path.extension().is_none_or(|e| e != "html") {
            continue;
        }
        files += 1;
        let html = fs::read_to_string(path).map_err(io_err(path))?;
        let rel = path.strip_prefix(out).unwrap().to_path_buf();
        let mut seen: HashSet<String> = HashSet::new();
        for link in extract_links(&html) {
            if !seen.insert(link.clone()) {
                continue; // report each distinct link once per page
            }
            let Some(target) = internal_target(&link) else {
                continue;
            };
            links += 1;
            let resolved = resolve(&rel, &target);
            let ok = match resolved {
                None => false,
                Some(r) => {
                    exists_cache.contains(&r)
                        || if resolves_to_file(out, &r) {
                            exists_cache.insert(r);
                            true
                        } else {
                            false
                        }
                }
            };
            if !ok {
                problems.push(Problem {
                    page: rel.clone(),
                    link: link.clone(),
                    reason: "target not found in the built site".into(),
                });
            }
        }
    }
    Ok(CheckReport {
        build,
        files_checked: files,
        links_checked: links,
        problems,
    })
}

/// All `href`/`src` attribute values in a document.
pub fn extract_links(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lower = html.to_ascii_lowercase();
    for attr in ["href", "src"] {
        let mut from = 0;
        while let Some(pos) = lower[from..].find(attr) {
            let at = from + pos;
            from = at + attr.len();
            let before_ok = at == 0 || lower.as_bytes()[at - 1].is_ascii_whitespace();
            let rest = &html[from..];
            let rest_trim = rest.trim_start();
            let Some(rest_eq) = rest_trim.strip_prefix('=') else {
                continue;
            };
            let value = rest_eq.trim_start();
            let Some(quote) = value.chars().next().filter(|c| *c == '"' || *c == '\'') else {
                continue;
            };
            if !before_ok {
                continue;
            }
            if let Some(end) = value[1..].find(quote) {
                out.push(value[1..1 + end].to_string());
            }
        }
    }
    out
}

/// The path part of a link if it points inside the site; `None` for external or fragment-only links.
fn internal_target(link: &str) -> Option<String> {
    let link = link.trim();
    let link = match link.strip_prefix(CHECK_ORIGIN) {
        Some("") => "/",
        Some(rest) => rest,
        None => link,
    };
    if link.is_empty() || link.starts_with('#') || link.starts_with("//") {
        return None;
    }
    let head = link.split(['/', '?', '#']).next().unwrap_or("");
    if head.contains(':') {
        return None; // http:, https:, mailto:, tel:, data:, javascript:
    }
    let path = link.split(['?', '#']).next().unwrap_or("");
    if path.is_empty() {
        return None;
    }
    Some(percent_decode(path))
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Resolve `target` against the page it appears on, as a path relative to the build root.
fn resolve(page_rel: &Path, target: &str) -> Option<PathBuf> {
    let mut parts: Vec<String> = Vec::new();
    if !target.starts_with('/') {
        if let Some(dir) = page_rel.parent() {
            parts.extend(
                dir.components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned()),
            );
        }
    }
    for seg in target.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s.to_string()),
        }
    }
    Some(parts.iter().collect())
}

fn resolves_to_file(out: &Path, rel: &Path) -> bool {
    let p = out.join(rel);
    p.is_file() || p.join("index.html").is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links() {
        let html = r#"<a href="/a/">x</a> <img src='b.png'> <link rel=stylesheet href="https://x.y/z"> data-href="no""#;
        let l = extract_links(html);
        assert!(l.contains(&"/a/".to_string()));
        assert!(l.contains(&"b.png".to_string()));
        assert!(!l.iter().any(|s| s == "no"));
    }

    #[test]
    fn targets() {
        assert_eq!(internal_target("https://x.com"), None);
        assert_eq!(
            internal_target("http://meteorite-check.invalid/a/"),
            Some("/a/".into())
        );
        assert_eq!(internal_target("mailto:a@b.c"), None);
        assert_eq!(internal_target("#top"), None);
        assert_eq!(internal_target("/a/b/#c"), Some("/a/b/".into()));
        assert_eq!(internal_target("../x.png?v=1"), Some("../x.png".into()));
        assert_eq!(internal_target("/a%20b/"), Some("/a b/".into()));
        // Malformed or non-ASCII escapes must not panic.
        assert_eq!(internal_target("/a%é/"), Some("/a%é/".into()));
        assert_eq!(internal_target("/100%/"), Some("/100%/".into()));
        assert_eq!(internal_target("/%a"), Some("/%a".into()));
    }

    #[test]
    fn resolving() {
        let page = Path::new("blog/post/index.html");
        assert_eq!(resolve(page, "/x/y/"), Some(PathBuf::from("x/y")));
        assert_eq!(
            resolve(page, "../other/"),
            Some(PathBuf::from("blog/other"))
        );
        assert_eq!(resolve(page, "../../../../x"), None);
    }
}
