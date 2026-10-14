//! Everything written to the output folder: collecting static files, assets and feeds, writing
//! only the files that changed, and removing leftovers from an earlier build.

use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use rayon::prelude::*;
use walkdir::WalkDir;

use crate::config::Config;
use crate::error::{io_err, Error, Result};
use crate::markdown::{self};
use crate::site::{Page, Site};
use crate::util::is_absolute_url;
use crate::{feed, minify};

pub(super) enum Source {
    Bytes(Vec<u8>),
    Copy(PathBuf),
}

/// Gathers every file of the output folder except feeds, the sitemap and robots.txt.
/// Later inserts win: theme static < site static < content assets < rendered pages.
pub(super) fn collect_outputs(
    root: &Path,
    theme_dir: &Path,
    config: &Config,
    site: &Site,
    chat_widget: Option<&crate::chat::BundledWidget>,
    knowledge_docs: &[crate::knowledge::KnowledgeDoc],
    rendered: Vec<Result<(PathBuf, Vec<u8>)>>,
) -> Result<HashMap<PathBuf, Source>> {
    let mut outputs: HashMap<PathBuf, Source> = HashMap::new();
    if let Some(w) = chat_widget {
        outputs.insert(PathBuf::from(&w.path), Source::Bytes(w.bytes.clone()));
        outputs.insert(
            PathBuf::from(w.license_path),
            Source::Bytes(w.license.clone().into_bytes()),
        );
    }
    let k = &config.chat.knowledge;
    if k.publish {
        outputs.insert(
            PathBuf::from(&k.publish_path),
            Source::Bytes(crate::knowledge::published_json(config, knowledge_docs).into_bytes()),
        );
    }
    if config.markdown.highlight {
        // Always generated, so a theme can link it unconditionally; a file of the same
        // name in the theme's or site's static/ overrides it.
        let css = markdown::highlight_css(
            &config.markdown.highlight_theme,
            &config.markdown.highlight_theme_dark,
        );
        outputs.insert("highlight.css".into(), Source::Bytes(css.into_bytes()));
    }
    collect_files(&theme_dir.join("static"), Path::new(""), &mut outputs)?;
    collect_files(&root.join("static"), Path::new(""), &mut outputs)?;
    collect_content_assets(&root.join("content"), site, &mut outputs)?;
    for r in rendered {
        let (path, bytes) = r?;
        outputs.insert(path, Source::Bytes(bytes));
    }
    Ok(outputs)
}

/// Adds the Atom and RSS feeds, sitemap.xml, robots.txt and the search index. Feeds and the
/// sitemap need an absolute `base_url`; a warning explains when they are skipped.
pub(super) fn add_feeds_and_robots(
    outputs: &mut HashMap<PathBuf, Source>,
    config: &Config,
    site: &Site,
    warnings: &mut Vec<String>,
) {
    let absolute = is_absolute_url(&config.base_url);
    let has_dated = site
        .all
        .iter()
        .any(|p| !p.is_index && p.date_parsed.is_some());
    if ((config.feed.enabled && has_dated) || config.sitemap) && !absolute {
        warnings.push(format!(
            "base_url `{}` is not absolute: feeds and sitemap.xml were skipped (set e.g. `base_url: https://example.com/`)",
            config.base_url
        ));
    }
    if absolute {
        if config.feed.enabled && has_dated {
            outputs.insert(
                "atom.xml".into(),
                Source::Bytes(feed::atom(config, site).into()),
            );
            outputs.insert(
                "rss.xml".into(),
                Source::Bytes(feed::rss(config, site).into()),
            );
        }
        if config.sitemap {
            outputs.insert(
                "sitemap.xml".into(),
                Source::Bytes(feed::sitemap(site).into()),
            );
        }
    }
    if !outputs.contains_key(Path::new("robots.txt")) {
        let robots = feed::robots(config, absolute && config.sitemap);
        outputs.insert("robots.txt".into(), Source::Bytes(robots.into()));
    }
    if config.search.enabled {
        outputs.insert(
            "search-index.json".into(),
            Source::Bytes(feed::search_index(config, site).into()),
        );
    }
}

/// Writes every output file in parallel and returns (written, unchanged) counts. A file whose
/// content is already on disk is left untouched.
pub(super) fn write_outputs(
    out_dir: &Path,
    outputs: &HashMap<PathBuf, Source>,
    minify_on: bool,
) -> Result<(usize, usize)> {
    let items: Vec<(&PathBuf, &Source)> = outputs.iter().collect();
    let results: Vec<Result<bool>> = items
        .par_iter()
        .map(|(rel, src)| write_output(out_dir, rel, src, minify_on))
        .collect();
    let mut written = 0;
    let mut unchanged = 0;
    for r in results {
        if r? {
            written += 1;
        } else {
            unchanged += 1;
        }
    }
    Ok((written, unchanged))
}

/// Marker written into every output directory Meteorite creates. A non-empty directory
/// without it is never wiped, so `--output ~/Documents` cannot destroy anything.
pub const OUTPUT_MARKER: &str = ".meteorite-output";

pub(super) fn prepare_output_dir(root: &Path, out_dir: &Path) -> Result<()> {
    if out_dir.exists() {
        let o = out_dir.canonicalize().map_err(io_err(out_dir))?;
        let r = root.canonicalize().map_err(io_err(root))?;
        // Never delete the site root, or an ancestor of it, even if misconfigured.
        if r.starts_with(&o) {
            return Err(Error::Msg(format!(
                "refusing to use {} as the output directory: it contains the site source",
                out_dir.display()
            )));
        }
        let empty = fs::read_dir(out_dir)
            .map_err(io_err(out_dir))?
            .next()
            .is_none();
        if !empty && !out_dir.join(OUTPUT_MARKER).is_file() {
            return Err(Error::Msg(format!(
                "refusing to overwrite {}: it is not empty and was not created by Meteorite (no {OUTPUT_MARKER} marker). \
                 Choose an empty directory or delete it yourself.",
                out_dir.display()
            )));
        }
        fs::remove_dir_all(out_dir).map_err(io_err(out_dir))?;
    }
    fs::create_dir_all(out_dir).map_err(io_err(out_dir))?;
    let marker = out_dir.join(OUTPUT_MARKER);
    fs::write(
        &marker,
        "Created by Meteorite. This directory is deleted and regenerated on build.\n",
    )
    .map_err(io_err(&marker))
}

pub(super) fn prune_empty_dirs(mut dir: Option<&Path>, stop: &Path) {
    while let Some(d) = dir {
        if d == stop || !d.starts_with(stop) {
            break;
        }
        if fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

pub(super) fn collect_files(
    dir: &Path,
    prefix: &Path,
    out: &mut HashMap<PathBuf, Source>,
) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in WalkDir::new(dir).follow_links(false) {
        let entry = entry.map_err(|e| Error::Io {
            path: dir.to_path_buf(),
            source: e.into(),
        })?;
        if !entry.file_type().is_file() || is_hidden(entry.path(), dir) {
            continue;
        }
        let rel = prefix.join(entry.path().strip_prefix(dir).unwrap());
        out.insert(rel, Source::Copy(entry.path().to_path_buf()));
    }
    Ok(())
}

pub(super) fn is_hidden(path: &Path, base: &Path) -> bool {
    path.strip_prefix(base)
        .unwrap_or(path)
        .components()
        .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
}

/// Non-Markdown files in `content/` are published next to their page: for a bundle
/// (`post/index.md`) that is the page's URL, otherwise the same relative path.
pub(super) fn collect_content_assets(
    content_dir: &Path,
    site: &Site,
    out: &mut HashMap<PathBuf, Source>,
) -> Result<()> {
    let bundles: HashMap<PathBuf, &Page> = site
        .all
        .iter()
        .filter(|p| p.is_bundle)
        .filter_map(|p| p.source.parent().map(|d| (d.to_path_buf(), p)))
        .collect();
    for entry in WalkDir::new(content_dir).follow_links(false) {
        let entry = entry.map_err(|e| Error::Io {
            path: content_dir.to_path_buf(),
            source: e.into(),
        })?;
        let path = entry.path();
        if !entry.file_type().is_file()
            || path.extension().is_some_and(|e| e == "md")
            || is_hidden(path, content_dir)
        {
            continue;
        }
        let rel = path.strip_prefix(content_dir).unwrap();
        let dest = match rel.parent().and_then(|d| bundles.get(d)) {
            Some(page) => Path::new(page.url.trim_matches('/')).join(rel.file_name().unwrap()),
            None => rel.to_path_buf(),
        };
        out.insert(dest, Source::Copy(path.to_path_buf()));
    }
    Ok(())
}

pub(super) fn write_output(
    out_dir: &Path,
    rel: &Path,
    src: &Source,
    minify_on: bool,
) -> Result<bool> {
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(Error::InvalidPath(format!("output path {}", rel.display())));
    }
    let dest = out_dir.join(rel);
    let ext = rel.extension().and_then(|e| e.to_str()).unwrap_or("");
    let minifiable = minify_on && matches!(ext, "html" | "css");

    let bytes: Option<Vec<u8>> = match src {
        Source::Bytes(b) => Some(b.clone()),
        Source::Copy(p) if minifiable => Some(fs::read(p).map_err(io_err(p))?),
        Source::Copy(_) => None,
    };

    if let Some(mut bytes) = bytes {
        if minifiable {
            if let Ok(text) = std::str::from_utf8(&bytes) {
                bytes = match ext {
                    "html" => minify::html(text),
                    _ => minify::css(text),
                }
                .into_bytes();
            }
        }
        if fs::read(&dest).is_ok_and(|existing| existing == bytes) {
            return Ok(false);
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(io_err(parent))?;
        }
        write_atomic(&dest, &bytes)?;
        return Ok(true);
    }

    let Source::Copy(from) = src else {
        unreachable!()
    };
    if let (Ok(a), Ok(b)) = (fs::metadata(from), fs::metadata(&dest)) {
        if a.len() == b.len() && matches!((a.modified(), b.modified()), (Ok(x), Ok(y)) if y >= x) {
            return Ok(false);
        }
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(io_err(parent))?;
    }
    let tmp = temp_path(&dest);
    fs::copy(from, &tmp).map_err(io_err(&tmp))?;
    fs::rename(&tmp, &dest).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        io_err(&dest)(e)
    })?;
    Ok(true)
}

pub(super) fn temp_path(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    dest.with_file_name(format!(".{name}.meteorite-tmp"))
}

/// Write via a temporary file and rename, so a reader (the dev server, a CDN sync) never
/// sees a half-written file.
pub(super) fn write_atomic(dest: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = temp_path(dest);
    fs::write(&tmp, bytes).map_err(io_err(&tmp))?;
    fs::rename(&tmp, dest).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        io_err(dest)(e)
    })
}
