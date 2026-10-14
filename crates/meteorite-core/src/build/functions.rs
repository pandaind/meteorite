//! Template-facing pieces: loading the theme's and site's templates, the `url()`, `get_section()`,
//! `get_taxonomy()` and related functions, and the `data/` folder.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use tera::{Tera, Value};
use walkdir::WalkDir;

use crate::config::Config;
use crate::content::FrontMatter;
use crate::error::{io_err, tera_err, Error, Result};
use crate::markdown::TocEntry;
use crate::site::{PageLight, Site};
use crate::util::{hash_parts, join_url};

/// Theme templates load first; the site's own `templates/` override them by name.
/// Returns the engine, a hash of every template source, and whether any template calls
/// `chat_widget()`.
pub(super) fn load_templates(root: &Path, theme_dir: &Path) -> Result<(Tera, u64, bool)> {
    let mut tera = Tera::default();
    let mut hasher_parts: Vec<String> = Vec::new();
    let mut uses_chat = false;
    for base in [theme_dir.join("templates"), root.join("templates")] {
        if !base.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        for entry in WalkDir::new(&base).sort_by_file_name() {
            let entry = entry.map_err(|e| Error::Io {
                path: base.clone(),
                source: e.into(),
            })?;
            if entry.file_type().is_file() {
                let name = entry
                    .path()
                    .strip_prefix(&base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                if let Ok(text) = fs::read_to_string(entry.path()) {
                    uses_chat |= text.contains("chat_widget(");
                    hasher_parts.push(format!("{name}\0{text}"));
                }
                files.push((entry.path().to_path_buf(), Some(name)));
            }
        }
        tera.add_template_files(files).map_err(tera_err)?;
    }
    let refs: Vec<&str> = hasher_parts.iter().map(String::as_str).collect();
    Ok((tera, hash_parts(&refs), uses_chat))
}

pub(super) fn register_url_fn(tera: &mut Tera, base_url: &str) {
    let base = base_url.to_string();
    tera.register_function("url", move |args: &HashMap<String, Value>| {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| tera::Error::msg("url(): `path` is required"))?;
        Ok(Value::String(join_url(&base, path)))
    });
    // A default avatar for a name: `{{ identicon(seed=config.title) | safe }}`.
    tera.register_function("identicon", |args: &HashMap<String, Value>| {
        let seed = args
            .get("seed")
            .and_then(Value::as_str)
            .ok_or_else(|| tera::Error::msg("identicon(): `seed` is required"))?;
        Ok(Value::String(crate::identicon::svg(seed)))
    });
}

pub(super) struct FnData {
    /// `(section path, page)`, newest first.
    pages: Vec<(String, Value)>,
    /// Section path -> that section's pages in display order.
    section_pages: BTreeMap<String, Vec<Value>>,
    sections: BTreeMap<String, Value>,
}

#[derive(Serialize)]
pub(super) struct SubsectionView<'a> {
    path: &'a str,
    url: &'a str,
    title: &'a str,
    description: &'a Option<String>,
}

#[derive(Serialize)]
pub(super) struct SectionView<'a> {
    path: &'a str,
    url: &'a str,
    permalink: String,
    title: &'a str,
    description: &'a Option<String>,
    meta: &'a FrontMatter,
    content: &'a str,
    toc: &'a [TocEntry],
    subsections: Vec<SubsectionView<'a>>,
    pages: Vec<PageLight<'a>>,
    total_pages: usize,
}

pub(super) fn section_view<'a>(
    site: &'a Site,
    config: &Config,
    path: &str,
    page_slice: &[usize],
) -> SectionView<'a> {
    let sec = &site.sections[path];
    let index = sec.index.map(|i| &site.all[i]);
    SectionView {
        path: &sec.path,
        url: &sec.url,
        permalink: join_url(&config.base_url, &sec.url),
        title: &sec.title,
        description: &sec.meta.description,
        meta: &sec.meta,
        content: index.map(|p| p.content.as_str()).unwrap_or(""),
        toc: index.map(|p| p.toc.as_slice()).unwrap_or(&[]),
        subsections: sec
            .subsections
            .iter()
            .map(|p| {
                let s = &site.sections[p];
                SubsectionView {
                    path: &s.path,
                    url: &s.url,
                    title: &s.title,
                    description: &s.meta.description,
                }
            })
            .collect(),
        pages: page_slice.iter().map(|&i| site.listing(i)).collect(),
        total_pages: sec.pages.len(),
    }
}

pub(super) fn register_site_fns(tera: &mut Tera, site: &Site, config: &Config) {
    let mut pages: Vec<(String, Value)> = Vec::new();
    let mut order: Vec<usize> = (0..site.all.len())
        .filter(|&i| !site.all[i].is_index)
        .collect();
    order.sort_by(|&a, &b| {
        site.all[b]
            .date_parsed
            .cmp(&site.all[a].date_parsed)
            .then_with(|| site.all[a].url.cmp(&site.all[b].url))
    });
    for i in order {
        let p = &site.all[i];
        pages.push((
            p.section.clone(),
            tera::to_value(site.listing(i)).unwrap_or(Value::Null),
        ));
    }
    let mut section_pages = BTreeMap::new();
    let mut sections = BTreeMap::new();
    for (path, sec) in &site.sections {
        section_pages.insert(
            path.clone(),
            sec.pages
                .iter()
                .map(|&i| tera::to_value(site.listing(i)).unwrap_or(Value::Null))
                .collect::<Vec<_>>(),
        );
        let view = section_view(site, config, path, &[]);
        sections.insert(path.clone(), tera::to_value(view).unwrap_or(Value::Null));
    }
    let data = Arc::new(FnData {
        pages,
        section_pages,
        sections,
    });

    let d = data.clone();
    tera.register_function("get_pages", move |args: &HashMap<String, Value>| {
        let section = args
            .get("section")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim_matches('/');
        let recursive = args
            .get("recursive")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .map(|n| n as usize)
            .unwrap_or(usize::MAX);
        let prefix = format!("{section}/");
        let out: Vec<Value> = d
            .pages
            .iter()
            .filter(|(s, _)| {
                s == section || (recursive && (section.is_empty() || s.starts_with(&prefix)))
            })
            .take(limit)
            .map(|(_, v)| v.clone())
            .collect();
        Ok(Value::Array(out))
    });

    let d = data;
    tera.register_function("get_section", move |args: &HashMap<String, Value>| {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim_matches('/');
        let mut v = d
            .sections
            .get(path)
            .cloned()
            .ok_or_else(|| tera::Error::msg(format!("get_section(): no section `{path}`")))?;
        if let Some(obj) = v.as_object_mut() {
            obj.insert(
                "pages".into(),
                Value::Array(d.section_pages.get(path).cloned().unwrap_or_default()),
            );
        }
        Ok(v)
    });
}

pub(super) fn load_data_dir(dir: &Path) -> Result<BTreeMap<String, serde_norway::Value>> {
    let mut out = BTreeMap::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    for entry in WalkDir::new(dir).max_depth(1).sort_by_file_name() {
        let entry = entry.map_err(|e| Error::Io {
            path: dir.to_path_buf(),
            source: e.into(),
        })?;
        let path = entry.path();
        let is_yaml = path.extension().is_some_and(|e| e == "yml" || e == "yaml");
        if entry.file_type().is_file() && is_yaml {
            let text = fs::read_to_string(path).map_err(io_err(path))?;
            let value: serde_norway::Value =
                serde_norway::from_str(&text).map_err(|e| Error::yaml(path, &text, 0, &e))?;
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                out.insert(stem.to_string(), value);
            }
        }
    }
    Ok(out)
}
