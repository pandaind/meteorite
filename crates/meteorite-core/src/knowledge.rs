//! Export a built site as a knowledge base for poptalk-rag.
//!
//! Each published page becomes one Markdown file with YAML front matter (title, url, date, tags,
//! section). Shortcodes are replaced by their plain text and the home page's front-matter blocks
//! (hero, features, ...) become prose, so nothing a visitor can read is missing from the index.
//! Only files this exporter created are ever removed or overwritten.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_norway::Value;
use sha2::{Digest, Sha256};
use tera::{Context, Tera};

use crate::config::Config;
use crate::error::{io_err, tera_err, Error, Result};
use crate::shortcode::{self, Arg};
use crate::site::{Page, Site};
use crate::util::strip_tags;

const MANIFEST: &str = ".meteorite-export";

#[derive(Debug, Clone)]
pub struct ExportReport {
    pub dir: PathBuf,
    pub written: usize,
    pub unchanged: usize,
    pub removed: usize,
}

/// One exportable page: everything a retrieval index needs, independent of the output format.
#[derive(Debug, Clone)]
pub struct KnowledgeDoc {
    /// File name relative to the persona folder, for example `guide/start.md`.
    pub path: String,
    pub title: String,
    /// Absolute public URL of the page.
    pub url: String,
    pub section: String,
    pub date: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    /// Content path of the source file, for example `guide/start.md`.
    pub source: String,
    /// Markdown body (a title heading, the description, the page text and home-page blocks).
    pub body: String,
}

impl KnowledgeDoc {
    /// The document as a Markdown file with YAML front matter, as poptalk-rag ingests it.
    fn to_file_text(&self) -> Result<String> {
        let mut front: BTreeMap<&str, Value> = BTreeMap::new();
        front.insert("title", Value::String(self.title.clone()));
        front.insert("url", Value::String(self.url.clone()));
        front.insert("section", Value::String(self.section.clone()));
        front.insert("source", Value::String(self.source.clone()));
        if let Some(d) = &self.date {
            front.insert("date", Value::String(d.clone()));
        }
        if let Some(d) = &self.description {
            front.insert("description", Value::String(d.clone()));
        }
        if !self.tags.is_empty() {
            front.insert(
                "tags",
                Value::Sequence(self.tags.iter().cloned().map(Value::String).collect()),
            );
        }
        let yaml = serde_norway::to_string(&front).map_err(|e| Error::Msg(e.to_string()))?;
        Ok(format!("---\n{yaml}---\n\n{}", self.body))
    }

    fn content_hash(&self) -> String {
        let digest = Sha256::digest(self.body.as_bytes());
        format!(
            "sha256:{}",
            digest
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )
    }
}

/// Collect the published pages that pass `include` / `exclude` / `chat: false`.
pub fn collect(config: &Config, site: &Site, tera: &Tera) -> Result<Vec<KnowledgeDoc>> {
    let k = &config.chat.knowledge;
    let mut docs = Vec::new();
    for page in &site.all {
        if !wanted(page, &k.include, &k.exclude) {
            continue;
        }
        docs.push(document(page, tera)?);
    }
    docs.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(docs)
}

/// Version of the published JSON layout. poptalk-rag refuses versions it does not know.
pub const MANIFEST_VERSION: u64 = 1;

/// The single-file form of the knowledge base, for `chat.knowledge.publish`. Deterministic:
/// no timestamps, keys sorted, documents ordered by path, so unchanged content is byte-identical.
pub fn published_json(config: &Config, docs: &[KnowledgeDoc]) -> String {
    use tera::Value as J;
    let mut digest_input = String::new();
    let documents: Vec<J> = docs
        .iter()
        .map(|d| {
            let hash = d.content_hash();
            digest_input.push_str(&format!("{}\0{}\n", d.path, hash));
            let mut m = tera::Map::new();
            m.insert("path".into(), J::String(d.path.clone()));
            m.insert("title".into(), J::String(d.title.clone()));
            m.insert("url".into(), J::String(d.url.clone()));
            m.insert("section".into(), J::String(d.section.clone()));
            if let Some(date) = &d.date {
                m.insert("date".into(), J::String(date.clone()));
            }
            if let Some(desc) = &d.description {
                m.insert("description".into(), J::String(desc.clone()));
            }
            m.insert(
                "tags".into(),
                J::Array(d.tags.iter().cloned().map(J::String).collect()),
            );
            m.insert("hash".into(), J::String(hash));
            m.insert("content".into(), J::String(d.body.clone()));
            J::Object(m)
        })
        .collect();
    let digest = Sha256::digest(digest_input.as_bytes());
    let mut top = tera::Map::new();
    top.insert("version".into(), J::from(MANIFEST_VERSION));
    top.insert(
        "generator".into(),
        J::String(format!("meteorite {}", env!("CARGO_PKG_VERSION"))),
    );
    top.insert("site".into(), J::String(config.base_url.clone()));
    top.insert(
        "digest".into(),
        J::String(format!(
            "sha256:{}",
            digest
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )),
    );
    top.insert("documents".into(), J::Array(documents));
    J::Object(top).to_string()
}

/// Write `docs` into `chat.knowledge.dir` if it is set. Returns `None` when it is not.
pub fn export(root: &Path, config: &Config, docs: &[KnowledgeDoc]) -> Result<Option<ExportReport>> {
    let k = &config.chat.knowledge;
    if k.dir.trim().is_empty() {
        return Ok(None);
    }
    let dir = resolve_dir(root, &k.dir)?;

    // What we wrote last time, so that we only ever touch our own files.
    let manifest_path = dir.join(MANIFEST);
    let previous: BTreeSet<String> = fs::read_to_string(&manifest_path)
        .map(|t| {
            t.lines()
                .filter(|l| !l.trim().is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    let mut current: BTreeSet<String> = BTreeSet::new();
    let (mut written, mut unchanged) = (0, 0);
    for doc in docs {
        let rel = doc.path.clone();
        let text = doc.to_file_text()?;
        let target = dir.join(&rel);
        if let Ok(existing) = fs::read_to_string(&target) {
            if existing == text {
                unchanged += 1;
                current.insert(rel);
                continue;
            }
            if !previous.contains(&rel) {
                return Err(Error::Msg(format!(
                    "refusing to overwrite {}: it already exists and was not created by Meteorite",
                    target.display()
                )));
            }
        } else if target.exists() {
            return Err(Error::Msg(format!(
                "refusing to overwrite {}: it exists and is not a text file Meteorite created",
                target.display()
            )));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(io_err(parent))?;
        }
        write_atomic(&target, text.as_bytes())?;
        written += 1;
        current.insert(rel);
    }

    let mut removed = 0;
    for stale in previous.difference(&current) {
        let path = dir.join(stale);
        if path.starts_with(&dir) && path.is_file() && fs::remove_file(&path).is_ok() {
            removed += 1;
            let mut parent = path.parent();
            while let Some(d) = parent {
                if d == dir || fs::remove_dir(d).is_err() {
                    break;
                }
                parent = d.parent();
            }
        }
    }

    let listing: String = current.iter().map(|p| format!("{p}\n")).collect();
    write_atomic(&manifest_path, listing.as_bytes())?;
    Ok(Some(ExportReport {
        dir,
        written,
        unchanged,
        removed,
    }))
}

/// The export directory must be somewhere else than the site's own source and output.
fn resolve_dir(root: &Path, configured: &str) -> Result<PathBuf> {
    let p = Path::new(configured);
    let dir = if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    };
    fs::create_dir_all(&dir).map_err(io_err(&dir))?;
    let dir = dir.canonicalize().map_err(io_err(&dir))?;
    let root_c = root.canonicalize().map_err(io_err(root))?;
    let bad = |why: &str| {
        Error::Msg(format!(
            "config.yml: chat.knowledge.dir `{configured}` {why}; choose a directory outside the site, \
             such as poptalk-rag's knowledge/<persona>"
        ))
    };
    if root_c.starts_with(&dir) {
        return Err(bad("contains the site itself"));
    }
    for owned in ["content", "themes", "templates", "static", "data", "public"] {
        if dir.starts_with(root_c.join(owned)) {
            return Err(bad(&format!("is inside the site's `{owned}/` directory")));
        }
    }
    Ok(dir)
}

fn wanted(page: &Page, include: &[String], exclude: &[String]) -> bool {
    if page.meta.draft {
        return false;
    }
    if matches!(page.meta.extra.get("chat"), Some(Value::Bool(false))) {
        return false;
    }
    let under = |prefixes: &[String]| {
        prefixes.iter().any(|p| {
            let p = p.trim_matches('/');
            p.is_empty() || page.section == p || page.section.starts_with(&format!("{p}/"))
        })
    };
    if !include.is_empty() && !under(include) {
        return false;
    }
    !under(exclude)
}

/// `/` -> `index.md`, `/guide/` -> `guide/index.md`, `/guide/start/` -> `guide/start.md`.
fn relative_path(page: &Page) -> String {
    let trimmed = page.url.trim_matches('/');
    if trimmed.is_empty() {
        "index.md".to_string()
    } else if page.is_index {
        format!("{trimmed}/index.md")
    } else {
        format!("{trimmed}.md")
    }
}

fn document(page: &Page, tera: &Tera) -> Result<KnowledgeDoc> {
    let description = page
        .meta
        .description
        .as_deref()
        .filter(|d| !d.is_empty())
        .map(str::to_string);

    let mut body = format!("# {}\n\n", page.meta.title);
    if let Some(d) = &description {
        body.push_str(d);
        body.push_str("\n\n");
    }
    body.push_str(plain_body(page, tera)?.trim());
    body.push('\n');
    let blocks = front_matter_prose(&page.meta.extra);
    if !blocks.is_empty() {
        body.push('\n');
        body.push_str(&blocks);
    }

    Ok(KnowledgeDoc {
        path: relative_path(page),
        title: page.meta.title.clone(),
        url: page.permalink.clone(),
        section: page.section.clone(),
        date: page.date.clone(),
        description,
        tags: page.meta.tags.clone(),
        source: page.source.display().to_string().replace('\\', "/"),
        body,
    })
}

/// The page body with shortcodes replaced by their text content.
fn plain_body(page: &Page, tera: &Tera) -> Result<String> {
    shortcode::expand(&page.body, &mut |name, args, inner| {
        let template = format!("shortcodes/{name}.html");
        if !tera.get_template_names().any(|n| n == template) {
            return Ok(inner.unwrap_or_default().to_string());
        }
        let mut ctx = Context::new();
        for (k, v) in args {
            match v {
                Arg::Str(s) => ctx.insert(k, s),
                Arg::Int(i) => ctx.insert(k, i),
                Arg::Float(f) => ctx.insert(k, f),
                Arg::Bool(b) => ctx.insert(k, b),
            }
        }
        // Keep the inner Markdown as written; the shortcode's wrapper markup is dropped.
        ctx.insert("body", inner.unwrap_or_default());
        let html = tera.render(&template, &ctx).map_err(tera_err)?;
        Ok(strip_tags(&html))
    })
    .map_err(|e| match e {
        Error::Msg(m) => Error::content(&page.file, m),
        other => other,
    })
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key)?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn items<'a>(v: &'a Value, key: &str) -> Vec<&'a Value> {
    v.get(key)
        .and_then(Value::as_sequence)
        .map(|s| s.iter().collect())
        .unwrap_or_default()
}

/// Home-page blocks used by the bundled themes (`hero`, `stats`, `features`, `showcase`, `quote`,
/// `cta`) hold real copy. Turn them into Markdown so that it can be searched.
fn front_matter_prose(extra: &BTreeMap<String, Value>) -> String {
    let mut out = String::new();
    let mut para = |s: &str| {
        out.push_str(s);
        out.push_str("\n\n");
    };
    if let Some(h) = extra.get("hero") {
        if let Some(t) = text(h, "title") {
            para(&format!("## {t}"));
        }
        for k in ["eyebrow", "subtitle"] {
            if let Some(s) = text(h, k) {
                para(&s);
            }
        }
        if let Some(c) = text(h, "command") {
            para(&format!("Get started: `{c}`"));
        }
    }
    if let Some(Value::Sequence(stats)) = extra.get("stats") {
        let lines: Vec<String> = stats
            .iter()
            .filter_map(|s| match (text(s, "value"), text(s, "label")) {
                (Some(v), Some(l)) => Some(format!("- {v} {l}")),
                (Some(v), None) => Some(format!("- {v}")),
                _ => None,
            })
            .collect();
        if !lines.is_empty() {
            para(&lines.join("\n"));
        }
    }
    if let Some(f) = extra.get("features") {
        if let Some(t) = text(f, "title") {
            para(&format!("## {t}"));
        }
        if let Some(s) = text(f, "subtitle") {
            para(&s);
        }
        for item in items(f, "items") {
            if let Some(t) = text(item, "title") {
                para(&format!("### {t}"));
            }
            if let Some(s) = text(item, "text") {
                para(&s);
            }
        }
    }
    if let Some(s) = extra.get("showcase") {
        if let Some(t) = text(s, "title") {
            para(&format!("## {t}"));
        }
        if let Some(t) = text(s, "text") {
            para(&t);
        }
        let points: Vec<String> = items(s, "points")
            .iter()
            .filter_map(|p| p.as_str().map(|p| format!("- {p}")))
            .collect();
        if !points.is_empty() {
            para(&points.join("\n"));
        }
    }
    if let Some(q) = extra.get("quote") {
        if let Some(t) = text(q, "text") {
            let who = match (text(q, "name"), text(q, "role")) {
                (Some(n), Some(r)) => format!(" ({n}, {r})"),
                (Some(n), None) => format!(" ({n})"),
                _ => String::new(),
            };
            para(&format!("> {t}{who}"));
        }
    }
    if let Some(c) = extra.get("cta") {
        if let Some(t) = text(c, "title") {
            para(&format!("## {t}"));
        }
        if let Some(t) = text(c, "text") {
            para(&t);
        }
    }
    out.trim_end().to_string() + if out.is_empty() { "" } else { "\n" }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.meteorite-tmp"));
    fs::write(&tmp, bytes).map_err(io_err(&tmp))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        io_err(path)(e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hero_and_features_become_prose() {
        let y: BTreeMap<String, Value> = serde_norway::from_str(
            "hero: { title: Static sites, subtitle: Fast and plain, command: cargo install meteorite }\n\
             stats: [ { value: '~1 s', label: 'to build 10,000 pages' } ]\n\
             features: { title: Why, items: [ { title: Plain files, text: No database. } ] }\n\
             cta: { title: Start, text: Install it. }\n",
        )
        .unwrap();
        let p = front_matter_prose(&y);
        for want in [
            "## Static sites",
            "Fast and plain",
            "`cargo install meteorite`",
            "- ~1 s to build 10,000 pages",
            "### Plain files",
            "No database.",
            "## Start",
        ] {
            assert!(p.contains(want), "missing {want:?} in:\n{p}");
        }
    }

    #[test]
    fn urls_map_to_stable_file_names() {
        let mk = |url: &str, is_index: bool| {
            let mut p = Page::synthetic("t", url, "https://x.test/");
            p.is_index = is_index;
            relative_path(&p)
        };
        assert_eq!(mk("/", true), "index.md");
        assert_eq!(mk("/guide/", true), "guide/index.md");
        assert_eq!(mk("/guide/start/", false), "guide/start.md");
    }

    #[test]
    fn include_exclude_and_per_page_opt_out() {
        let page = |section: &str| {
            let mut p = Page::synthetic("t", "/x/", "/");
            p.section = section.to_string();
            p
        };
        assert!(wanted(&page("guide"), &[], &[]));
        assert!(wanted(&page("guide/deep"), &["guide".into()], &[]));
        assert!(!wanted(&page("blog"), &["guide".into()], &[]));
        assert!(!wanted(
            &page("guide/private"),
            &[],
            &["guide/private".into()]
        ));
        let mut optout = page("guide");
        optout.meta.extra.insert("chat".into(), Value::Bool(false));
        assert!(!wanted(&optout, &[], &[]));
        let mut draft = page("guide");
        draft.meta.draft = true;
        assert!(!wanted(&draft, &[], &[]));
    }
}
