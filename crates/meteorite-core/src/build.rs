//! The build pipeline: config -> content -> site model -> render -> write.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::ops::Range;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use rayon::prelude::*;
use serde::Serialize;
use tera::{Context, Tera, Value};
use walkdir::WalkDir;

use crate::config::{Config, HighlightStyle};
use crate::content::{self, FrontMatter};
use crate::error::{io_err, tera_err, Error, Result};
use crate::markdown::{self, MarkdownOptions, TocEntry};
use crate::shortcode::{self, Arg};
use crate::site::{self, url_to_path, ModelOptions, Page, PageLight, Site};
use crate::theme::Theme;
use crate::util::{hash_parts, is_absolute_url, join_url, slugify};
use crate::{feed, images, minify};

#[derive(Debug, Clone, Default)]
pub struct BuildOptions {
    /// Include pages marked `draft: true`.
    pub drafts: bool,
    /// Include pages dated in the future.
    pub future: bool,
    /// Override `build.minify` from config.
    pub minify: Option<bool>,
    /// Override `base_url` (used by `serve` and `check`).
    pub base_url: Option<String>,
    /// Write somewhere other than `<root>/public`.
    pub out_dir: Option<PathBuf>,
    /// Treat this as today's date (for tests).
    pub today: Option<NaiveDate>,
}

#[derive(Debug, Clone)]
pub struct BuildReport {
    pub pages: usize,
    pub sections: usize,
    pub files_written: usize,
    pub files_unchanged: usize,
    pub files_removed: usize,
    pub skipped_drafts: usize,
    pub skipped_future: usize,
    pub skipped_expired: usize,
    pub warnings: Vec<String>,
    pub output: PathBuf,
    pub elapsed: Duration,
}

/// One-shot build of the site at `root`.
pub fn build(root: &Path, opts: &BuildOptions) -> Result<BuildReport> {
    Builder::new(root).build(opts)
}

#[derive(Clone)]
struct Cached {
    content: String,
    summary: String,
    toc: Vec<TocEntry>,
    word_count: usize,
    reading_time: usize,
}

enum Source {
    Bytes(Vec<u8>),
    Copy(PathBuf),
}

enum Job {
    Page(usize),
    Section {
        path: String,
        number: usize,
        range: Range<usize>,
        total: usize,
    },
    TaxonomyIndex(String),
    Term {
        taxonomy: String,
        index: usize,
    },
    NotFound,
}

/// A reusable builder. Keeping one around between builds (as `serve` does) lets it
/// skip re-rendering unchanged Markdown and leave unchanged output files untouched.
pub struct Builder {
    root: PathBuf,
    cache: HashMap<PathBuf, (u64, Cached)>,
    last_outputs: HashSet<PathBuf>,
    last_out_dir: Option<PathBuf>,
}

impl Builder {
    pub fn new(root: &Path) -> Self {
        Builder {
            root: root.to_path_buf(),
            cache: HashMap::new(),
            last_outputs: HashSet::new(),
            last_out_dir: None,
        }
    }

    pub fn build(&mut self, opts: &BuildOptions) -> Result<BuildReport> {
        let start = Instant::now();
        let root = self.root.clone();
        let mut warnings = Vec::new();

        let mut config = Config::load(&root)?;
        if let Some(b) = &opts.base_url {
            config.base_url = b.clone();
        }
        if let Some(m) = opts.minify {
            config.build.minify = m;
        }
        let drafts = opts.drafts || config.build.drafts;
        let future = opts.future || config.build.future;

        let theme_dir = root.join("themes").join(&config.theme);
        if !theme_dir.is_dir() {
            return Err(Error::ThemeNotFound(config.theme.clone()));
        }
        let theme = Theme::load(&theme_dir)?;
        theme.resolve_params(&mut config.params)?;

        let out_dir = opts.out_dir.clone().unwrap_or_else(|| root.join("public"));
        let first_build = self.last_out_dir.as_deref() != Some(out_dir.as_path());
        if first_build {
            self.last_outputs.clear();
            prepare_output_dir(&root, &out_dir)?;
        }

        // Templates and functions that don't depend on the site model.
        let (mut tera, templates_hash) = load_templates(&root, &theme_dir)?;
        tera.set_escape_fn(crate::util::html_escape);
        register_url_fn(&mut tera, &config.base_url);
        let generated: images::Generated = Arc::new(Mutex::new(Vec::new()));
        images::register(
            &mut tera,
            &root,
            &out_dir,
            &config.base_url,
            generated.clone(),
        );

        // Content -> model.
        let raws = content::load_all(&root.join("content"))?;
        let today = opts
            .today
            .unwrap_or_else(|| chrono::Utc::now().date_naive());
        let mut site = site::build_model(
            &config,
            raws,
            &ModelOptions {
                drafts,
                future,
                today,
            },
        )?;

        // Markdown (parallel, cached).
        if config.markdown.highlight {
            for name in [
                &config.markdown.highlight_theme,
                &config.markdown.highlight_theme_dark,
            ] {
                if !markdown::is_highlight_theme(name) {
                    let mut known = markdown::highlight_theme_names();
                    known.sort();
                    return Err(Error::Msg(format!(
                        "config.yml: unknown highlight theme `{name}`. Available: {}",
                        known.join(", ")
                    )));
                }
            }
        }
        let md_opts = MarkdownOptions {
            highlight: config.markdown.highlight,
            highlight_theme: config.markdown.highlight_theme.clone(),
            classed: config.markdown.highlight_style == HighlightStyle::Classes,
        };
        let md_sig = format!(
            "{}|{}|{}|{}|{}",
            md_opts.highlight,
            md_opts.highlight_theme,
            md_opts.classed,
            config.base_url,
            templates_hash
        );
        let results: Vec<Result<(u64, Cached)>> = {
            let cache = &self.cache;
            site.all
                .par_iter()
                .map(|p| {
                    let key = hash_parts(&[&md_sig, &p.body]);
                    if let Some((k, c)) = cache.get(&p.file) {
                        if *k == key {
                            return Ok((key, c.clone()));
                        }
                    }
                    render_markdown(&tera, &md_opts, p).map(|c| (key, c))
                })
                .collect()
        };
        let mut new_cache = HashMap::with_capacity(results.len());
        for (page, res) in site.all.iter_mut().zip(results) {
            let (key, c) = res?;
            page.content = c.content.clone();
            page.summary = c.summary.clone();
            page.toc = c.toc.clone();
            page.word_count = c.word_count;
            page.reading_time = c.reading_time;
            new_cache.insert(page.file.clone(), (key, c));
        }
        self.cache = new_cache;

        // Template data and functions that need the model.
        let data = load_data_dir(&root.join("data"))?;
        register_site_fns(&mut tera, &site, &config);

        let mut base = Context::new();
        base.insert("config", &config);
        base.insert("data", &data);
        base.insert(
            "meteorite",
            &MeteoriteInfo {
                version: env!("CARGO_PKG_VERSION"),
            },
        );

        // Jobs.
        let mut jobs: Vec<Job> = (0..site.all.len())
            .filter(|&i| !site.all[i].is_index)
            .map(Job::Page)
            .collect();
        for sec in site.sections.values() {
            let per = sec.meta.paginate_by.or(config.paginate_by);
            for (n, range) in paginate(sec.pages.len(), per).into_iter().enumerate() {
                jobs.push(Job::Section {
                    path: sec.path.clone(),
                    number: n + 1,
                    range,
                    total: paginate(sec.pages.len(), per).len(),
                });
            }
        }
        for (tax, terms) in &site.taxonomies {
            jobs.push(Job::TaxonomyIndex(tax.clone()));
            for index in 0..terms.len() {
                jobs.push(Job::Term {
                    taxonomy: tax.clone(),
                    index,
                });
            }
        }
        if tera.get_template_names().any(|n| n == "404.html") {
            jobs.push(Job::NotFound);
        }

        let rendered: Vec<Result<(PathBuf, Vec<u8>)>> = jobs
            .par_iter()
            .map(|job| render_job(job, &tera, &site, &config, &base))
            .collect();

        // Collect outputs. Later inserts win: theme static < site static < assets < pages.
        let mut outputs: HashMap<PathBuf, Source> = HashMap::new();
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
        collect_content_assets(&root.join("content"), &site, &mut outputs)?;
        for r in rendered {
            let (path, bytes) = r?;
            outputs.insert(path, Source::Bytes(bytes));
        }

        // Feeds, sitemap, robots, search.
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
                    Source::Bytes(feed::atom(&config, &site).into()),
                );
                outputs.insert(
                    "rss.xml".into(),
                    Source::Bytes(feed::rss(&config, &site).into()),
                );
            }
            if config.sitemap {
                outputs.insert(
                    "sitemap.xml".into(),
                    Source::Bytes(feed::sitemap(&site).into()),
                );
            }
        }
        if !outputs.contains_key(Path::new("robots.txt")) {
            let robots = feed::robots(&config, absolute && config.sitemap);
            outputs.insert("robots.txt".into(), Source::Bytes(robots.into()));
        }
        if config.search.enabled {
            outputs.insert(
                "search-index.json".into(),
                Source::Bytes(feed::search_index(&config, &site).into()),
            );
        }

        // Files created directly by template functions (processed images).
        let generated: Vec<PathBuf> = generated.lock().unwrap().clone();

        // Write.
        let minify_on = config.build.minify;
        let items: Vec<(&PathBuf, &Source)> = outputs.iter().collect();
        let results: Vec<Result<bool>> = items
            .par_iter()
            .map(|(rel, src)| write_output(&out_dir, rel, src, minify_on))
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

        // Remove files that an earlier build produced but this one didn't.
        let mut now: HashSet<PathBuf> = outputs.keys().cloned().collect();
        now.extend(generated);
        let mut removed = 0;
        for stale in self.last_outputs.difference(&now) {
            let path = out_dir.join(stale);
            if path.is_file() && fs::remove_file(&path).is_ok() {
                removed += 1;
                prune_empty_dirs(path.parent(), &out_dir);
            }
        }
        self.last_outputs = now;
        self.last_out_dir = Some(out_dir.clone());

        Ok(BuildReport {
            pages: site.all.len(),
            sections: site.sections.len(),
            files_written: written,
            files_unchanged: unchanged,
            files_removed: removed,
            skipped_drafts: site.skipped_drafts,
            skipped_future: site.skipped_future,
            skipped_expired: site.skipped_expired,
            warnings,
            output: out_dir,
            elapsed: start.elapsed(),
        })
    }
}

#[derive(Serialize)]
struct MeteoriteInfo {
    version: &'static str,
}

/// Marker written into every output directory Meteorite creates. A non-empty directory
/// without it is never wiped, so `--output ~/Documents` cannot destroy anything.
pub const OUTPUT_MARKER: &str = ".meteorite-output";

fn prepare_output_dir(root: &Path, out_dir: &Path) -> Result<()> {
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

fn prune_empty_dirs(mut dir: Option<&Path>, stop: &Path) {
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

/// Theme templates load first; the site's own `templates/` override them by name.
fn load_templates(root: &Path, theme_dir: &Path) -> Result<(Tera, u64)> {
    let mut tera = Tera::default();
    let mut hasher_parts: Vec<String> = Vec::new();
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
                    hasher_parts.push(format!("{name}\0{text}"));
                }
                files.push((entry.path().to_path_buf(), Some(name)));
            }
        }
        tera.add_template_files(files).map_err(tera_err)?;
    }
    let refs: Vec<&str> = hasher_parts.iter().map(String::as_str).collect();
    Ok((tera, hash_parts(&refs)))
}

fn register_url_fn(tera: &mut Tera, base_url: &str) {
    let base = base_url.to_string();
    tera.register_function("url", move |args: &HashMap<String, Value>| {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| tera::Error::msg("url(): `path` is required"))?;
        Ok(Value::String(join_url(&base, path)))
    });
}

struct FnData {
    /// `(section path, page)`, newest first.
    pages: Vec<(String, Value)>,
    /// Section path -> that section's pages in display order.
    section_pages: BTreeMap<String, Vec<Value>>,
    sections: BTreeMap<String, Value>,
}

#[derive(Serialize)]
struct SubsectionView<'a> {
    path: &'a str,
    url: &'a str,
    title: &'a str,
    description: &'a Option<String>,
}

#[derive(Serialize)]
struct SectionView<'a> {
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

fn section_view<'a>(
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
        pages: page_slice.iter().map(|&i| site.all[i].light()).collect(),
        total_pages: sec.pages.len(),
    }
}

fn register_site_fns(tera: &mut Tera, site: &Site, config: &Config) {
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
            tera::to_value(p.light()).unwrap_or(Value::Null),
        ));
    }
    let mut section_pages = BTreeMap::new();
    let mut sections = BTreeMap::new();
    for (path, sec) in &site.sections {
        section_pages.insert(
            path.clone(),
            sec.pages
                .iter()
                .map(|&i| tera::to_value(site.all[i].light()).unwrap_or(Value::Null))
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

fn load_data_dir(dir: &Path) -> Result<BTreeMap<String, serde_norway::Value>> {
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

fn render_markdown(tera: &Tera, opts: &MarkdownOptions, page: &Page) -> Result<Cached> {
    let expanded = shortcode::expand(&page.body, &mut |name, args, inner| {
        let template = format!("shortcodes/{name}.html");
        if !tera.get_template_names().any(|n| n == template) {
            return Err(Error::Msg(format!(
                "unknown shortcode `{name}` (no templates/{template})"
            )));
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
        if let Some(inner) = inner {
            ctx.insert(
                "body",
                &markdown::render(inner.trim_matches('\n'), opts).html,
            );
        }
        tera.render(&template, &ctx).map_err(tera_err)
    })
    .map_err(|e| match e {
        Error::Msg(m) => Error::content(&page.file, m),
        other => other,
    })?;

    let rendered = markdown::render(&expanded, opts);
    let summary = match rendered.html.find("<!-- more -->") {
        Some(at) => rendered.html[..at].trim().to_string(),
        None => match rendered.html.find("<p>") {
            Some(start) => match rendered.html[start..].find("</p>") {
                Some(end) => rendered.html[start..start + end + 4].to_string(),
                None => String::new(),
            },
            None => String::new(),
        },
    };
    let words = page.body.split_whitespace().count();
    Ok(Cached {
        content: rendered.html,
        summary,
        toc: rendered.toc,
        word_count: words,
        reading_time: if words == 0 {
            0
        } else {
            words.div_ceil(200).max(1)
        },
    })
}

/// Split `total` items into pages. Always returns at least one (possibly empty) range.
fn paginate(total: usize, per: Option<usize>) -> Vec<Range<usize>> {
    match per {
        Some(n) if n > 0 && total > n => (0..total.div_ceil(n))
            .map(|i| i * n..((i + 1) * n).min(total))
            .collect(),
        #[allow(clippy::single_range_in_vec_init)]
        _ => vec![0..total],
    }
}

#[derive(Serialize)]
struct Paginator {
    number: usize,
    total_pages: usize,
    per_page: usize,
    total_items: usize,
    current_url: String,
    first_url: String,
    last_url: String,
    prev_url: Option<String>,
    next_url: Option<String>,
}

fn page_url(section_url: &str, number: usize) -> String {
    if number <= 1 {
        section_url.to_string()
    } else {
        format!("{section_url}page/{number}/")
    }
}

#[derive(Serialize)]
struct ParentRef<'a> {
    path: &'a str,
    url: &'a str,
    title: &'a str,
}

#[derive(Serialize)]
struct TaxonomyView<'a> {
    name: &'a str,
    url: String,
}

#[derive(Serialize)]
struct TermView<'a> {
    name: &'a str,
    slug: &'a str,
    url: &'a str,
    count: usize,
}

fn render_job(
    job: &Job,
    tera: &Tera,
    site: &Site,
    config: &Config,
    base: &Context,
) -> Result<(PathBuf, Vec<u8>)> {
    let mut ctx = base.clone();
    let (template, out_path): (String, PathBuf) = match job {
        Job::Page(i) => {
            let page = &site.all[*i];
            let parent = &site.sections[&page.section];
            ctx.insert("page", page);
            ctx.insert(
                "section",
                &ParentRef {
                    path: &parent.path,
                    url: &parent.url,
                    title: &parent.title,
                },
            );
            (
                page.meta
                    .template
                    .clone()
                    .unwrap_or_else(|| "page.html".into()),
                page.output_path(),
            )
        }
        Job::Section {
            path,
            number,
            range,
            total,
        } => {
            let sec = &site.sections[path];
            let per = sec.meta.paginate_by.or(config.paginate_by);
            let slice = &sec.pages[range.clone()];
            ctx.insert("section", &section_view(site, config, path, slice));
            match sec.index {
                Some(i) => ctx.insert("page", &site.all[i]),
                None => ctx.insert(
                    "page",
                    &Page::synthetic(&sec.title, &sec.url, &config.base_url),
                ),
            }
            ctx.insert(
                "paginator",
                &Paginator {
                    number: *number,
                    total_pages: *total,
                    per_page: per.unwrap_or(sec.pages.len()),
                    total_items: sec.pages.len(),
                    current_url: page_url(&sec.url, *number),
                    first_url: page_url(&sec.url, 1),
                    last_url: page_url(&sec.url, *total),
                    prev_url: (*number > 1).then(|| page_url(&sec.url, number - 1)),
                    next_url: (number < total).then(|| page_url(&sec.url, number + 1)),
                },
            );
            let default = if path.is_empty() {
                "index.html"
            } else {
                "section.html"
            };
            (
                sec.meta.template.clone().unwrap_or_else(|| default.into()),
                url_to_path(&page_url(&sec.url, *number)),
            )
        }
        Job::TaxonomyIndex(tax) => {
            let terms = &site.taxonomies[tax];
            let url = format!("/{}/", slugify(tax));
            ctx.insert(
                "taxonomy",
                &TaxonomyView {
                    name: tax,
                    url: url.clone(),
                },
            );
            ctx.insert(
                "terms",
                &terms
                    .iter()
                    .map(|t| TermView {
                        name: &t.name,
                        slug: &t.slug,
                        url: &t.url,
                        count: t.pages.len(),
                    })
                    .collect::<Vec<_>>(),
            );
            ctx.insert("page", &Page::synthetic(tax, &url, &config.base_url));
            ("taxonomy.html".into(), url_to_path(&url))
        }
        Job::Term { taxonomy, index } => {
            let term = &site.taxonomies[taxonomy][*index];
            let tax_url = format!("/{}/", slugify(taxonomy));
            ctx.insert(
                "taxonomy",
                &TaxonomyView {
                    name: taxonomy,
                    url: tax_url,
                },
            );
            ctx.insert(
                "term",
                &TermView {
                    name: &term.name,
                    slug: &term.slug,
                    url: &term.url,
                    count: term.pages.len(),
                },
            );
            ctx.insert(
                "pages",
                &term
                    .pages
                    .iter()
                    .map(|&i| site.all[i].light())
                    .collect::<Vec<_>>(),
            );
            ctx.insert(
                "page",
                &Page::synthetic(&term.name, &term.url, &config.base_url),
            );
            ("term.html".into(), url_to_path(&term.url))
        }
        Job::NotFound => {
            ctx.insert(
                "page",
                &Page::synthetic("Page not found", "/404.html", &config.base_url),
            );
            ("404.html".into(), PathBuf::from("404.html"))
        }
    };
    let html = tera.render(&template, &ctx).map_err(|e| {
        let what = match job {
            Job::Page(i) => site.all[*i].source.display().to_string(),
            Job::Section { path, .. } => format!("section `{path}`"),
            Job::TaxonomyIndex(t) => format!("taxonomy `{t}`"),
            Job::Term { taxonomy, .. } => format!("taxonomy `{taxonomy}`"),
            Job::NotFound => "404".to_string(),
        };
        match tera_err(e) {
            Error::Template(m) => Error::Template(format!("rendering {what} with {template}: {m}")),
            other => other,
        }
    })?;
    Ok((out_path, html.into_bytes()))
}

fn collect_files(dir: &Path, prefix: &Path, out: &mut HashMap<PathBuf, Source>) -> Result<()> {
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

fn is_hidden(path: &Path, base: &Path) -> bool {
    path.strip_prefix(base)
        .unwrap_or(path)
        .components()
        .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
}

/// Non-Markdown files in `content/` are published next to their page: for a bundle
/// (`post/index.md`) that is the page's URL, otherwise the same relative path.
fn collect_content_assets(
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

fn write_output(out_dir: &Path, rel: &Path, src: &Source, minify_on: bool) -> Result<bool> {
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

fn temp_path(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    dest.with_file_name(format!(".{name}.meteorite-tmp"))
}

/// Write via a temporary file and rename, so a reader (the dev server, a CDN sync) never
/// sees a half-written file.
fn write_atomic(dest: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = temp_path(dest);
    fs::write(&tmp, bytes).map_err(io_err(&tmp))?;
    fs::rename(&tmp, dest).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        io_err(dest)(e)
    })
}
