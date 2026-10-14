//! Turning the site model into HTML: which pages and listings to render, the template
//! context each kind gets, pagination, and the error shown when a template fails.

use std::fs;
use std::ops::Range;
use std::path::PathBuf;

use serde::Serialize;
use tera::{Context, Tera};
use walkdir::WalkDir;

use crate::config::Config;
use crate::error::{tera_err, Error, Result};
use crate::site::{url_to_path, Page, Site};
use crate::util::slugify;

use super::functions::section_view;

/// Lists everything that has to be rendered: single pages, every page of each section
/// listing, taxonomy indexes and terms, and the 404 page when the theme has one.
pub(super) fn plan_jobs(site: &Site, config: &Config, tera: &Tera) -> Vec<Job> {
    let mut jobs: Vec<Job> = (0..site.all.len())
        .filter(|&i| !site.all[i].is_index)
        .map(Job::Page)
        .collect();
    for sec in site.sections.values() {
        let per = sec.meta.paginate_by.or(config.paginate_by);
        let ranges = paginate(sec.pages.len(), per);
        let total = ranges.len();
        for (n, range) in ranges.into_iter().enumerate() {
            jobs.push(Job::Section {
                path: sec.path.clone(),
                number: n + 1,
                range,
                total,
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
    jobs
}

pub(super) enum Job {
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

/// Split `total` items into pages. Always returns at least one (possibly empty) range.
pub(super) fn paginate(total: usize, per: Option<usize>) -> Vec<Range<usize>> {
    match per {
        Some(n) if n > 0 && total > n => (0..total.div_ceil(n))
            .map(|i| i * n..((i + 1) * n).min(total))
            .collect(),
        #[allow(clippy::single_range_in_vec_init)]
        _ => vec![0..total],
    }
}

#[derive(Serialize)]
pub(super) struct Paginator {
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

pub(super) fn page_url(section_url: &str, number: usize) -> String {
    if number <= 1 {
        section_url.to_string()
    } else {
        format!("{section_url}page/{number}/")
    }
}

#[derive(Serialize)]
pub(super) struct ParentRef<'a> {
    path: &'a str,
    url: &'a str,
    title: &'a str,
}

#[derive(Serialize)]
pub(super) struct TaxonomyView<'a> {
    name: &'a str,
    url: String,
}

#[derive(Serialize)]
pub(super) struct TermView<'a> {
    name: &'a str,
    slug: &'a str,
    url: &'a str,
    count: usize,
}

/// What a job needs from a template: the template's name and where its output goes.
pub(super) type Target = (String, PathBuf);

/// Renders one page or listing. Each job kind first fills the template context with what its
/// template expects (`page`, `section`, `paginator`, `taxonomy`, ...), then the template runs.
pub(super) fn render_job(
    job: &Job,
    tera: &Tera,
    site: &Site,
    config: &Config,
    base: &Context,
    template_dirs: &[PathBuf],
) -> Result<(PathBuf, Vec<u8>)> {
    let mut ctx = base.clone();
    let (template, out_path) = match job {
        Job::Page(i) => prepare_page(&mut ctx, site, *i),
        Job::Section {
            path,
            number,
            range,
            total,
        } => prepare_section(&mut ctx, site, config, path, *number, range, *total),
        Job::TaxonomyIndex(tax) => prepare_taxonomy_index(&mut ctx, site, config, tax),
        Job::Term { taxonomy, index } => prepare_term(&mut ctx, site, config, taxonomy, *index),
        Job::NotFound => prepare_not_found(&mut ctx, config),
    };
    let html = tera
        .render(&template, &ctx)
        .map_err(|e| render_error(e, job, site, &template, template_dirs))?;
    Ok((out_path, html.into_bytes()))
}

/// A single page, with its section as `section`.
pub(super) fn prepare_page(ctx: &mut Context, site: &Site, index: usize) -> Target {
    let page = &site.all[index];
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

/// One page of a section listing, with `section`, `page` (the section's index page, or a
/// stand-in) and `paginator`.
pub(super) fn prepare_section(
    ctx: &mut Context,
    site: &Site,
    config: &Config,
    path: &str,
    number: usize,
    range: &Range<usize>,
    total: usize,
) -> Target {
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
            number,
            total_pages: total,
            per_page: per.unwrap_or(sec.pages.len()),
            total_items: sec.pages.len(),
            current_url: page_url(&sec.url, number),
            first_url: page_url(&sec.url, 1),
            last_url: page_url(&sec.url, total),
            prev_url: (number > 1).then(|| page_url(&sec.url, number - 1)),
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
        url_to_path(&page_url(&sec.url, number)),
    )
}

/// The page that lists every term of a taxonomy (for example `/tags/`).
pub(super) fn prepare_taxonomy_index(
    ctx: &mut Context,
    site: &Site,
    config: &Config,
    taxonomy: &str,
) -> Target {
    let terms = &site.taxonomies[taxonomy];
    let url = format!("/{}/", slugify(taxonomy));
    ctx.insert(
        "taxonomy",
        &TaxonomyView {
            name: taxonomy,
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
    ctx.insert("page", &Page::synthetic(taxonomy, &url, &config.base_url));
    ("taxonomy.html".into(), url_to_path(&url))
}

/// The page that lists the pages carrying one term (for example `/tags/rust/`).
pub(super) fn prepare_term(
    ctx: &mut Context,
    site: &Site,
    config: &Config,
    taxonomy: &str,
    index: usize,
) -> Target {
    let term = &site.taxonomies[taxonomy][index];
    ctx.insert(
        "taxonomy",
        &TaxonomyView {
            name: taxonomy,
            url: format!("/{}/", slugify(taxonomy)),
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
            .map(|&i| site.listing(i))
            .collect::<Vec<_>>(),
    );
    ctx.insert(
        "page",
        &Page::synthetic(&term.name, &term.url, &config.base_url),
    );
    ("term.html".into(), url_to_path(&term.url))
}

pub(super) fn prepare_not_found(ctx: &mut Context, config: &Config) -> Target {
    ctx.insert(
        "page",
        &Page::synthetic("Page not found", "/404.html", &config.base_url),
    );
    ("404.html".into(), PathBuf::from("404.html"))
}

/// Wraps a template failure with what was being rendered and, when it can be found, the line
/// of the template file that caused it.
pub(super) fn render_error(
    e: tera::Error,
    job: &Job,
    site: &Site,
    template: &str,
    template_dirs: &[PathBuf],
) -> Error {
    let what = match job {
        Job::Page(i) => site.all[*i].source.display().to_string(),
        Job::Section { path, .. } => format!("section `{path}`"),
        Job::TaxonomyIndex(t) => format!("taxonomy `{t}`"),
        Job::Term { taxonomy, .. } => format!("taxonomy `{taxonomy}`"),
        Job::NotFound => "404".to_string(),
    };
    match tera_err(e) {
        Error::Template(m) => {
            let hint = locate_in_templates(template_dirs, &m)
                .map(|(p, line)| format!(" (likely at {}:{line})", p.display()))
                .unwrap_or_default();
            Error::Template(format!("rendering {what} with {template}: {m}{hint}"))
        }
        other => other,
    }
}

/// Find where a render error probably came from: the first template line that mentions the
/// first backtick-quoted name in the message (for example the missing variable).
pub(super) fn locate_in_templates(dirs: &[PathBuf], message: &str) -> Option<(PathBuf, usize)> {
    let start = message.find('`')? + 1;
    let needle = &message[start..start + message[start..].find('`')?];
    if needle.is_empty() {
        return None;
    }
    // Try the full dotted name first, then its last segment.
    let last = needle.rsplit('.').next().unwrap_or(needle);
    for pattern in [needle, last] {
        for dir in dirs {
            for entry in WalkDir::new(dir).sort_by_file_name().into_iter().flatten() {
                if !entry.file_type().is_file() {
                    continue;
                }
                if let Ok(text) = fs::read_to_string(entry.path()) {
                    if let Some(i) = text.lines().position(|l| l.contains(pattern)) {
                        return Some((entry.path().to_path_buf(), i + 1));
                    }
                }
            }
        }
    }
    None
}
