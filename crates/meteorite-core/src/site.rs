//! The in-memory site model: pages, sections, taxonomies, ordering and URLs.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use serde::Serialize;

use crate::config::Config;
use crate::content::{parse_date, FrontMatter, RawPage};
use crate::error::{Error, Result};
use crate::markdown::TocEntry;
use crate::util::{humanize, join_url, slugify};

#[derive(Debug, Clone, Serialize)]
pub struct Link {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Page {
    pub meta: FrontMatter,
    /// Root-relative URL, always starting and ending with `/`.
    pub url: String,
    /// `base_url` + `url`.
    pub permalink: String,
    /// Rendered HTML body.
    pub content: String,
    /// HTML up to `<!-- more -->`, or the first paragraph.
    pub summary: String,
    /// Normalised `YYYY-MM-DD`, if the page has a valid date.
    pub date: Option<String>,
    pub word_count: usize,
    pub reading_time: usize,
    pub toc: Vec<TocEntry>,
    /// Path of the containing section (`""` is the site root).
    pub section: String,
    pub is_index: bool,
    pub prev: Option<Link>,
    pub next: Option<Link>,

    #[serde(skip)]
    pub body: String,
    #[serde(skip)]
    pub file: PathBuf,
    #[serde(skip)]
    pub source: PathBuf,
    #[serde(skip)]
    pub date_parsed: Option<NaiveDate>,
    /// Co-located assets live next to `index.md` and are copied beside the page.
    #[serde(skip)]
    pub is_bundle: bool,
}

/// A page without its body, cheap enough to put in lists.
#[derive(Debug, Serialize)]
pub struct PageLight<'a> {
    pub meta: &'a FrontMatter,
    pub url: &'a str,
    pub permalink: &'a str,
    pub summary: &'a str,
    pub date: &'a Option<String>,
    pub word_count: usize,
    pub reading_time: usize,
    pub section: &'a str,
    /// Only present for pages of a section that sets `full_content: true` in its `_index.md`
    /// (micro-blog streams that show whole posts in the listing).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<&'a str>,
}

impl Page {
    pub fn light(&self) -> PageLight<'_> {
        PageLight {
            meta: &self.meta,
            url: &self.url,
            permalink: &self.permalink,
            summary: &self.summary,
            date: &self.date,
            word_count: self.word_count,
            reading_time: self.reading_time,
            section: &self.section,
            content: None,
        }
    }

    /// Like [`Page::light`] but with the rendered body, for sections that opt in.
    pub fn light_full(&self) -> PageLight<'_> {
        PageLight {
            content: Some(&self.content),
            ..self.light()
        }
    }

    /// Output file relative to the output directory.
    pub fn output_path(&self) -> PathBuf {
        url_to_path(&self.url)
    }

    pub fn synthetic(title: &str, url: &str, base_url: &str) -> Page {
        Page {
            meta: FrontMatter {
                title: title.to_string(),
                ..Default::default()
            },
            url: url.to_string(),
            permalink: join_url(base_url, url),
            content: String::new(),
            summary: String::new(),
            date: None,
            word_count: 0,
            reading_time: 0,
            toc: Vec::new(),
            section: String::new(),
            is_index: false,
            prev: None,
            next: None,
            body: String::new(),
            file: PathBuf::new(),
            source: PathBuf::new(),
            date_parsed: None,
            is_bundle: false,
        }
    }
}

pub fn url_to_path(url: &str) -> PathBuf {
    let rel = url.trim_matches('/');
    if rel.is_empty() {
        PathBuf::from("index.html")
    } else {
        Path::new(rel).join("index.html")
    }
}

#[derive(Debug)]
pub struct Section {
    pub path: String,
    pub url: String,
    pub title: String,
    pub meta: FrontMatter,
    /// Index into `Site::all` of this section's `_index.md`, if it has one.
    pub index: Option<usize>,
    /// Regular pages in this section, in display order.
    pub pages: Vec<usize>,
    /// Paths of direct child sections.
    pub subsections: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Term {
    pub name: String,
    pub slug: String,
    pub url: String,
    /// Indices into `Site::all`, newest first.
    pub pages: Vec<usize>,
}

#[derive(Debug)]
pub struct Site {
    /// Every page, section indexes included.
    pub all: Vec<Page>,
    pub sections: BTreeMap<String, Section>,
    pub taxonomies: BTreeMap<String, Vec<Term>>,
    pub skipped_drafts: usize,
    pub skipped_future: usize,
    pub skipped_expired: usize,
}

impl Site {
    /// A page as it appears in a listing: without its body, unless the page's section sets
    /// `full_content: true` in its `_index.md`.
    pub fn listing(&self, i: usize) -> PageLight<'_> {
        let page = &self.all[i];
        let full = self
            .sections
            .get(&page.section)
            .and_then(|s| s.meta.extra.get("full_content"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if full {
            page.light_full()
        } else {
            page.light()
        }
    }
}

pub struct ModelOptions {
    pub drafts: bool,
    pub future: bool,
    pub today: NaiveDate,
}

/// Safe, non-empty path components (no `.`/`..`, separators or control characters).
fn slug_ok(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('.')
        && !s.contains("..")
        && s.chars()
            .all(|c| !c.is_control() && c != '/' && c != '\\' && c != ':')
}

fn section_url(path: &str) -> String {
    if path.is_empty() {
        "/".to_string()
    } else {
        format!("/{path}/")
    }
}

fn parent_of(path: &str) -> &str {
    path.rsplit_once('/').map(|(p, _)| p).unwrap_or("")
}

/// Where a source file sits in `content/`, and what kind of page that makes it.
struct Layout {
    /// File name without extension.
    stem: String,
    /// Folders between `content/` and the file.
    dir_comps: Vec<String>,
    /// `_index.md` (or a top-level `index.md`): the page that describes a section.
    is_section_index: bool,
    /// `post/index.md`: a page bundle, whose folder name is the slug.
    is_bundle: bool,
}

impl Layout {
    fn of(raw: &RawPage) -> Layout {
        let stem = raw
            .source
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let dir_comps: Vec<String> = raw
            .source
            .parent()
            .map(|p| {
                p.components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        let is_section_index = stem == "_index" || (stem == "index" && dir_comps.is_empty());
        let is_bundle = stem == "index" && !dir_comps.is_empty();
        Layout {
            stem,
            dir_comps,
            is_section_index,
            is_bundle,
        }
    }

    /// The section folders and the text the slug is derived from.
    fn section_and_slug_source(&self) -> (Vec<String>, String) {
        if self.is_section_index {
            (self.dir_comps.clone(), String::new())
        } else if self.is_bundle {
            let (last, parents) = self.dir_comps.split_last().unwrap();
            (parents.to_vec(), last.clone())
        } else {
            (self.dir_comps.clone(), self.stem.clone())
        }
    }
}

/// Why a page is left out of the build.
enum Skip {
    Draft,
    Future,
    Expired,
}

/// Decides whether a page is left out: drafts, future-dated pages and expired pages.
/// Section index pages are never skipped for their date.
fn skip_reason(
    raw: &RawPage,
    date: Option<NaiveDate>,
    is_section_index: bool,
    opts: &ModelOptions,
) -> Result<Option<Skip>> {
    if raw.meta.draft && !opts.drafts {
        return Ok(Some(Skip::Draft));
    }
    if is_section_index {
        return Ok(None);
    }
    if date.is_some_and(|d| d > opts.today) && !opts.future {
        return Ok(Some(Skip::Future));
    }
    if let Some(exp) = &raw.meta.expiry_date {
        let exp = parse_date(exp).ok_or_else(|| {
            Error::content(
                &raw.file,
                format!("`expiry_date: {exp}` is not a valid date"),
            )
        })?;
        if exp <= opts.today {
            return Ok(Some(Skip::Expired));
        }
    }
    Ok(None)
}

/// The URL of a page: the section's own URL for an index, otherwise the section plus a slug,
/// or the section's `permalinks` pattern when one applies. The slug is `slug:` from front
/// matter or is derived from the file (or bundle folder) name.
fn page_url(
    config: &Config,
    raw: &RawPage,
    layout: &Layout,
    section_path: &str,
    slug_source: &str,
    date: Option<NaiveDate>,
) -> Result<String> {
    if layout.is_section_index {
        return Ok(section_url(section_path));
    }
    let slug = match &raw.meta.slug {
        Some(s) => {
            if !slug_ok(s) {
                return Err(Error::InvalidPath(format!(
                    "{}: slug `{s}` may not be empty, start with `.`, or contain `/`, `\\`, `:` or `..`",
                    raw.file.display()
                )));
            }
            s.clone()
        }
        None => {
            let s = slugify(slug_source);
            if s.is_empty() {
                return Err(Error::InvalidPath(format!(
                    "{}: cannot derive a URL from `{slug_source}`; set `slug:` in front matter",
                    raw.file.display()
                )));
            }
            s
        }
    };
    match pattern_for(config, section_path) {
        Some(pattern) => expand_permalink(pattern, &slug, section_path, raw, date),
        None => Ok(format!("{}{slug}/", section_url(section_path))),
    }
}

/// Turns the loaded Markdown files into pages, then groups them into sections and taxonomies.
/// Pages still have empty `content`; Markdown is rendered later by the build.
pub fn build_model(config: &Config, raws: Vec<RawPage>, opts: &ModelOptions) -> Result<Site> {
    let mut site = Site {
        all: Vec::new(),
        sections: BTreeMap::new(),
        taxonomies: BTreeMap::new(),
        skipped_drafts: 0,
        skipped_future: 0,
        skipped_expired: 0,
    };
    let mut by_url: HashMap<String, PathBuf> = HashMap::new();

    for raw in raws {
        let layout = Layout::of(&raw);

        // Dates are checked first so a bad date is reported even for drafts.
        let date_parsed = match &raw.meta.date {
            Some(d) => Some(parse_date(d).ok_or_else(|| {
                Error::content(
                    &raw.file,
                    format!("`date: {d}` is not a valid date (use YYYY-MM-DD)"),
                )
            })?),
            None => None,
        };
        match skip_reason(&raw, date_parsed, layout.is_section_index, opts)? {
            Some(Skip::Draft) => {
                site.skipped_drafts += 1;
                continue;
            }
            Some(Skip::Future) => {
                site.skipped_future += 1;
                continue;
            }
            Some(Skip::Expired) => {
                site.skipped_expired += 1;
                continue;
            }
            None => {}
        }

        let (section_comps, slug_source) = layout.section_and_slug_source();
        let section_path = section_comps
            .iter()
            .map(|c| slugify(c))
            .filter(|c| !c.is_empty())
            .collect::<Vec<_>>()
            .join("/");
        let url = page_url(
            config,
            &raw,
            &layout,
            &section_path,
            &slug_source,
            date_parsed,
        )?;

        if let Some(prev) = by_url.insert(url.clone(), raw.file.clone()) {
            return Err(Error::UrlCollision {
                url,
                a: prev,
                b: raw.file,
            });
        }

        let mut meta = raw.meta;
        if meta.title.is_empty() {
            meta.title = if layout.is_section_index {
                section_comps
                    .last()
                    .map(|c| humanize(c))
                    .unwrap_or_else(|| config.title.clone())
            } else {
                humanize(&slug_source)
            };
        }
        let date = date_parsed.map(|d| d.format("%Y-%m-%d").to_string());
        site.all.push(Page {
            permalink: join_url(&config.base_url, &url),
            url,
            meta,
            content: String::new(),
            summary: String::new(),
            date,
            word_count: 0,
            reading_time: 0,
            toc: Vec::new(),
            section: section_path,
            is_index: layout.is_section_index,
            prev: None,
            next: None,
            body: raw.body,
            file: raw.file,
            source: raw.source,
            date_parsed,
            is_bundle: layout.is_bundle,
        });
    }

    build_sections(config, &mut site);
    build_taxonomies(config, &mut site);
    Ok(site)
}

fn pattern_for<'a>(config: &'a Config, section: &str) -> Option<&'a str> {
    let mut cur = section;
    loop {
        if let Some(p) = config.permalinks.get(cur) {
            return Some(p);
        }
        if cur.is_empty() {
            return None;
        }
        cur = parent_of(cur);
    }
}

fn expand_permalink(
    pattern: &str,
    slug: &str,
    section: &str,
    raw: &RawPage,
    date: Option<NaiveDate>,
) -> Result<String> {
    let needs_date = [":year", ":month", ":day"]
        .iter()
        .any(|t| pattern.contains(t));
    let date = match (needs_date, date) {
        (true, None) => {
            return Err(Error::content(
                &raw.file,
                format!("permalink `{pattern}` needs a `date:` in front matter"),
            ))
        }
        (_, d) => d,
    };
    let mut out = pattern.to_string();
    if let Some(d) = date {
        out = out
            .replace(":year", &d.format("%Y").to_string())
            .replace(":month", &d.format("%m").to_string())
            .replace(":day", &d.format("%d").to_string());
    }
    let title = slugify(&raw.meta.title);
    out = out
        .replace(":slug", slug)
        .replace(":section", section.rsplit('/').next().unwrap_or(""))
        .replace(":title", if title.is_empty() { slug } else { &title });
    if out.contains(':') && out.split('/').any(|seg| seg.starts_with(':')) {
        return Err(Error::content(
            &raw.file,
            format!("permalink `{pattern}` uses an unknown token (known: :year :month :day :slug :section :title)"),
        ));
    }
    let segs: Vec<&str> = out.split('/').filter(|s| !s.is_empty()).collect();
    if segs.iter().any(|s| *s == ".." || *s == ".") {
        return Err(Error::InvalidPath(format!(
            "{}: permalink expands to `{out}`",
            raw.file.display()
        )));
    }
    Ok(format!("/{}/", segs.join("/")))
}

fn ensure_section(site: &mut Site, config: &Config, path: &str) {
    if site.sections.contains_key(path) {
        return;
    }
    let title = if path.is_empty() {
        config.title.clone()
    } else {
        humanize(path.rsplit('/').next().unwrap())
    };
    site.sections.insert(
        path.to_string(),
        Section {
            path: path.to_string(),
            url: section_url(path),
            title,
            meta: FrontMatter::default(),
            index: None,
            pages: Vec::new(),
            subsections: Vec::new(),
        },
    );
    if !path.is_empty() {
        let parent = parent_of(path).to_string();
        ensure_section(site, config, &parent);
        site.sections
            .get_mut(&parent)
            .unwrap()
            .subsections
            .push(path.to_string());
    }
}

fn build_sections(config: &Config, site: &mut Site) {
    ensure_section(site, config, "");
    for i in 0..site.all.len() {
        let path = site.all[i].section.clone();
        ensure_section(site, config, &path);
        let is_index = site.all[i].is_index;
        let sec = site.sections.get_mut(&path).unwrap();
        if is_index {
            sec.index = Some(i);
            sec.meta = site.all[i].meta.clone();
            sec.title = site.all[i].meta.title.clone();
        } else {
            sec.pages.push(i);
        }
    }

    let all = &site.all;
    for sec in site.sections.values_mut() {
        let sort_by = sec.meta.sort_by.as_deref().unwrap_or("date");
        sec.pages.sort_by(|&a, &b| {
            let (pa, pb) = (&all[a], &all[b]);
            let ord = match sort_by {
                "weight" => pa.meta.weight.cmp(&pb.meta.weight),
                "title" => pa
                    .meta
                    .title
                    .to_lowercase()
                    .cmp(&pb.meta.title.to_lowercase()),
                // Newest first; undated pages sink to the end.
                _ => pb.date_parsed.cmp(&pa.date_parsed),
            };
            ord.then_with(|| pa.meta.weight.cmp(&pb.meta.weight))
                .then_with(|| pa.meta.title.cmp(&pb.meta.title))
                .then_with(|| pa.url.cmp(&pb.url))
        });
    }
    // Sub-sections: by weight, then title.
    let titles: HashMap<String, (i64, String)> = site
        .sections
        .iter()
        .map(|(k, s)| (k.clone(), (s.meta.weight, s.title.to_lowercase())))
        .collect();
    for sec in site.sections.values_mut() {
        sec.subsections.sort_by_key(|p| titles[p].clone());
    }

    // prev/next follow the section's display order.
    let links: Vec<(usize, Option<Link>, Option<Link>)> = site
        .sections
        .values()
        .flat_map(|sec| {
            sec.pages.iter().enumerate().map(|(pos, &idx)| {
                let link = |j: usize| Link {
                    title: all[j].meta.title.clone(),
                    url: all[j].url.clone(),
                };
                let prev = pos.checked_sub(1).map(|p| link(sec.pages[p]));
                let next = sec.pages.get(pos + 1).map(|&n| link(n));
                (idx, prev, next)
            })
        })
        .collect();
    for (idx, prev, next) in links {
        site.all[idx].prev = prev;
        site.all[idx].next = next;
    }
}

fn terms_of(meta: &FrontMatter, taxonomy: &str) -> Vec<String> {
    if taxonomy == "tags" {
        return meta.tags.clone();
    }
    match meta.extra.get(taxonomy) {
        Some(serde_norway::Value::Sequence(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        Some(serde_norway::Value::String(s)) => vec![s.clone()],
        _ => Vec::new(),
    }
}

fn build_taxonomies(config: &Config, site: &mut Site) {
    for tax in &config.taxonomies {
        let mut terms: BTreeMap<String, Term> = BTreeMap::new();
        for (idx, page) in site.all.iter().enumerate() {
            if page.is_index {
                continue;
            }
            for name in terms_of(&page.meta, tax) {
                let slug = slugify(&name);
                if slug.is_empty() {
                    continue;
                }
                terms
                    .entry(slug.clone())
                    .or_insert_with(|| Term {
                        name: name.clone(),
                        url: format!("/{}/{slug}/", slugify(tax)),
                        slug,
                        pages: Vec::new(),
                    })
                    .pages
                    .push(idx);
            }
        }
        let all = &site.all;
        let mut list: Vec<Term> = terms.into_values().collect();
        for t in &mut list {
            t.pages.sort_by(|&a, &b| {
                all[b]
                    .date_parsed
                    .cmp(&all[a].date_parsed)
                    .then_with(|| all[a].url.cmp(&all[b].url))
            });
            t.pages.dedup();
        }
        site.taxonomies.insert(tax.clone(), list);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn raw(path: &str, fm: &str) -> RawPage {
        let meta: FrontMatter = serde_norway::from_str(fm).unwrap();
        RawPage {
            meta,
            file: PathBuf::from("content").join(path),
            source: PathBuf::from(path),
            body: String::new(),
        }
    }

    fn opts() -> ModelOptions {
        ModelOptions {
            drafts: false,
            future: false,
            today: NaiveDate::from_ymd_opt(2026, 10, 10).unwrap(),
        }
    }

    #[test]
    fn urls_and_sections() {
        let cfg = Config::default();
        let site = build_model(
            &cfg,
            vec![
                raw("_index.md", "title: Home"),
                raw("about.md", "{}"),
                raw("blog/_index.md", "title: Blog"),
                raw("blog/first.md", "date: 2026-01-02"),
                raw("blog/second.md", "date: 2026-02-03\nslug: two"),
                raw("docs/guide/index.md", "{}"),
            ],
            &opts(),
        )
        .unwrap();
        let urls: Vec<_> = site.all.iter().map(|p| p.url.as_str()).collect();
        assert_eq!(
            urls,
            [
                "/",
                "/about/",
                "/blog/",
                "/blog/first/",
                "/blog/two/",
                "/docs/guide/"
            ]
        );
        let blog = &site.sections["blog"];
        // Newest first.
        assert_eq!(site.all[blog.pages[0]].url, "/blog/two/");
        assert_eq!(site.all[blog.pages[1]].url, "/blog/first/");
        // Bundle `docs/guide/index.md` belongs to section `docs`, which has no index.
        assert!(site.sections["docs"].index.is_none());
        assert_eq!(site.all[5].section, "docs");
        // prev/next
        assert_eq!(site.all[3].prev.as_ref().unwrap().url, "/blog/two/");
        assert!(site.all[3].next.is_none());
    }

    #[test]
    fn permalinks_and_collisions() {
        let cfg = Config {
            permalinks: BTreeMap::from([("blog".to_string(), "/:year/:month/:slug/".to_string())]),
            ..Default::default()
        };
        let site = build_model(&cfg, vec![raw("blog/a.md", "date: 2026-03-04")], &opts()).unwrap();
        assert_eq!(site.all[0].url, "/2026/03/a/");

        let err = build_model(&cfg, vec![raw("blog/a.md", "{}")], &opts()).unwrap_err();
        assert!(err.to_string().contains("needs a `date:`"));

        let plain = Config::default();
        let err = build_model(
            &plain,
            vec![raw("a.md", "slug: same"), raw("b.md", "slug: same")],
            &opts(),
        )
        .unwrap_err();
        assert!(matches!(err, Error::UrlCollision { .. }));
    }

    #[test]
    fn rejects_unsafe_slugs() {
        let cfg = Config::default();
        for bad in ["../x", "a/b", ".hidden", "a\\\\b"] {
            let fm = format!("slug: \"{bad}\"");
            let err = build_model(&cfg, vec![raw("a.md", &fm)], &opts()).unwrap_err();
            assert!(matches!(err, Error::InvalidPath(_)), "{bad}");
        }
    }

    #[test]
    fn drafts_future_and_expired() {
        let cfg = Config::default();
        let raws = || {
            vec![
                raw("d.md", "draft: true"),
                raw("f.md", "date: 2027-01-01"),
                raw("e.md", "expiry_date: 2026-01-01"),
                raw("ok.md", "date: 2026-01-01"),
            ]
        };
        let site = build_model(&cfg, raws(), &opts()).unwrap();
        assert_eq!(site.all.len(), 1);
        assert_eq!(
            (
                site.skipped_drafts,
                site.skipped_future,
                site.skipped_expired
            ),
            (1, 1, 1)
        );
        let all = ModelOptions {
            drafts: true,
            future: true,
            ..opts()
        };
        // Expired content stays hidden regardless of flags.
        assert_eq!(build_model(&cfg, raws(), &all).unwrap().all.len(), 3);
    }

    #[test]
    fn taxonomies() {
        let cfg = Config {
            taxonomies: vec!["tags".into(), "categories".into()],
            ..Default::default()
        };
        let site = build_model(
            &cfg,
            vec![
                raw(
                    "a.md",
                    "tags: [Rust, CMS]\ncategories: [Dev]\ndate: 2026-01-01",
                ),
                raw("b.md", "tags: [rust]\ndate: 2026-02-01"),
            ],
            &opts(),
        )
        .unwrap();
        let tags = &site.taxonomies["tags"];
        let rust = tags.iter().find(|t| t.slug == "rust").unwrap();
        assert_eq!(rust.pages.len(), 2);
        assert_eq!(rust.url, "/tags/rust/");
        assert_eq!(site.taxonomies["categories"].len(), 1);
    }
}
