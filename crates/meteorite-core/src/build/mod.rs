//! The build pipeline: config -> content -> site model -> render -> write.
//!
//! This file holds the pipeline itself (`Builder::build`, numbered in six steps) and the
//! Markdown cache. The pieces it calls live next to it:
//!
//! - `functions`: templates, template functions and the `data/` folder
//! - `render`: which pages and listings to render, and the context each template gets
//! - `outputs`: collecting, writing and cleaning up the output folder

mod functions;
mod outputs;
mod render;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use rayon::prelude::*;
use serde::Serialize;
use tera::{Context, Tera};

use crate::config::{Config, HighlightStyle};
use crate::content;
use crate::error::{tera_err, Error, Result};
use crate::images;
use crate::markdown::{self, MarkdownOptions, TocEntry};
use crate::shortcode::{self, Arg};
use crate::site::{self, ModelOptions, Page, Site};
use crate::theme::Theme;
use crate::util::hash_parts;

use functions::{load_data_dir, load_templates, register_site_fns, register_url_fn};
use outputs::{
    add_feeds_and_robots, collect_outputs, prepare_output_dir, prune_empty_dirs, write_outputs,
    Source,
};
use render::{plan_jobs, render_job};

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
    /// Build with this installed theme instead of the one in `config.yml`.
    pub theme: Option<String>,
    /// Write somewhere other than `<root>/public`.
    pub out_dir: Option<PathBuf>,
    /// Treat this as today's date (for tests).
    pub today: Option<NaiveDate>,
    /// Do not write the poptalk-rag knowledge export even if `chat.knowledge.dir` is set.
    /// `check` and `serve` set this so they never touch another project's files.
    pub skip_knowledge_export: bool,
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
    /// Present when `chat.knowledge.dir` is configured and the export ran.
    pub knowledge: Option<crate::knowledge::ExportReport>,
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

    /// Runs the whole pipeline once. Each numbered step below hands its result to the next;
    /// the helpers further down do the actual work.
    pub fn build(&mut self, opts: &BuildOptions) -> Result<BuildReport> {
        let start = Instant::now();
        let root = self.root.clone();
        let mut warnings = Vec::new();

        // 1. Settings and theme.
        let mut config = load_config(&root, opts, &mut warnings)?;
        let drafts = opts.drafts || config.build.drafts;
        let future = opts.future || config.build.future;

        let theme_dir = root.join("themes").join(&config.theme);
        if !theme_dir.is_dir() {
            return Err(Error::ThemeNotFound(config.theme.clone()));
        }
        let theme = Theme::load(&theme_dir)?;
        theme.resolve_params(&mut config.params)?;
        warn_about_theme(&config, &theme, &mut warnings);

        let out_dir = opts.out_dir.clone().unwrap_or_else(|| root.join("public"));
        let first_build = self.last_out_dir.as_deref() != Some(out_dir.as_path());
        if first_build {
            self.last_outputs.clear();
            prepare_output_dir(&root, &out_dir)?;
        }

        // 2. Templates, and the template functions that do not need the site model.
        let (mut tera, templates_hash, uses_chat) = load_templates(&root, &theme_dir)?;
        warn_if_chat_is_unused(&config, uses_chat, &mut warnings);
        let (chat_widget, generated_images) =
            register_core_functions(&mut tera, &root, &out_dir, &config);

        // 3. Content -> site model -> HTML.
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
        check_highlight_themes(&config)?;
        self.render_markdown(&tera, &config, templates_hash, &mut site)?;

        // 4. Render every page and listing.
        let data = load_data_dir(&root.join("data"))?;
        register_site_fns(&mut tera, &site, &config);

        let mut base = Context::new();
        base.insert("config", &config);
        base.insert("data", &data);
        base.insert("theme", &theme.manifest);
        base.insert(
            "meteorite",
            &MeteoriteInfo {
                version: env!("CARGO_PKG_VERSION"),
            },
        );

        // Site templates override the theme's, so search them first when locating errors.
        let template_dirs = [root.join("templates"), theme_dir.join("templates")];

        let jobs = plan_jobs(&site, &config, &tera);
        let rendered: Vec<Result<(PathBuf, Vec<u8>)>> = jobs
            .par_iter()
            .map(|job| render_job(job, &tera, &site, &config, &base, &template_dirs))
            .collect();

        // 5. Everything that ends up in the output folder.
        // The knowledge base is collected once and used for the directory export and/or the
        // published JSON file. Publishing is site output, so `serve` and `check` include it.
        let k = &config.chat.knowledge;
        let wants_dir_export = !k.dir.trim().is_empty() && !opts.skip_knowledge_export;
        let knowledge_docs = if wants_dir_export || k.publish {
            crate::knowledge::collect(&config, &site, &tera)?
        } else {
            Vec::new()
        };
        let mut outputs = collect_outputs(
            &root,
            &theme_dir,
            &config,
            &site,
            chat_widget.as_ref(),
            &knowledge_docs,
            rendered,
        )?;
        add_feeds_and_robots(&mut outputs, &config, &site, &mut warnings);

        // 6. Write, then remove what an earlier build produced but this one did not.
        let (written, unchanged) = write_outputs(&out_dir, &outputs, config.build.minify)?;
        let generated: Vec<PathBuf> = generated_images.lock().unwrap().clone();
        let removed = self.remove_stale_outputs(&outputs, generated, &out_dir);
        self.last_out_dir = Some(out_dir.clone());

        let knowledge = if wants_dir_export {
            crate::knowledge::export(&root, &config, &knowledge_docs)?
        } else {
            None
        };

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
            knowledge,
        })
    }

    /// Turns every page's Markdown into HTML in parallel, reusing the previous result when the
    /// page, the Markdown settings and the templates are unchanged.
    fn render_markdown(
        &mut self,
        tera: &Tera,
        config: &Config,
        templates_hash: u64,
        site: &mut Site,
    ) -> Result<()> {
        let md_opts = MarkdownOptions {
            highlight: config.markdown.highlight,
            highlight_theme: config.markdown.highlight_theme.clone(),
            classed: config.markdown.highlight_style == HighlightStyle::Classes,
        };
        // Anything that changes the HTML for the same Markdown belongs in this signature.
        let signature = format!(
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
                    let key = hash_parts(&[&signature, &p.body]);
                    if let Some((k, c)) = cache.get(&p.file) {
                        if *k == key {
                            return Ok((key, c.clone()));
                        }
                    }
                    render_markdown(tera, &md_opts, p).map(|c| (key, c))
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
        Ok(())
    }

    /// Deletes files that an earlier build wrote and this one did not, and remembers this
    /// build's file list for next time. Returns how many files were removed.
    fn remove_stale_outputs(
        &mut self,
        outputs: &HashMap<PathBuf, Source>,
        generated: Vec<PathBuf>,
        out_dir: &Path,
    ) -> usize {
        let mut now: HashSet<PathBuf> = outputs.keys().cloned().collect();
        now.extend(generated);
        let mut removed = 0;
        for stale in self.last_outputs.difference(&now) {
            let path = out_dir.join(stale);
            if path.is_file() && fs::remove_file(&path).is_ok() {
                removed += 1;
                prune_empty_dirs(path.parent(), out_dir);
            }
        }
        self.last_outputs = now;
        removed
    }
}

/// Loads `config.yml` and applies the command-line overrides.
fn load_config(root: &Path, opts: &BuildOptions, warnings: &mut Vec<String>) -> Result<Config> {
    let (mut config, config_warnings) = Config::load_with_warnings(root)?;
    warnings.extend(config_warnings);
    if let Some(b) = &opts.base_url {
        config.base_url = b.clone();
    }
    if let Some(m) = opts.minify {
        config.build.minify = m;
    }
    if let Some(t) = &opts.theme {
        crate::theme::check_name(t)?;
        config.theme = t.clone();
    }
    Ok(config)
}

/// Warns when the site's copy of the theme is older than the bundled one, or when
/// `params.skin` names a skin the theme does not have.
fn warn_about_theme(config: &Config, theme: &Theme, warnings: &mut Vec<String>) {
    if let Some(newer) = crate::theme::update_available(&config.theme, &theme.manifest.version) {
        warnings.push(format!(
            "theme `{}` is version {} but this release bundles {newer}; it is missing newer features. \
             Run `meteorite theme update {}` (your copy is kept in themes/.backup/)",
            config.theme, theme.manifest.version, config.theme
        ));
    }
    if let Some(serde_norway::Value::String(skin)) = config.params.get("skin") {
        let known: Vec<&str> = theme
            .manifest
            .skins
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        if !skin.is_empty() && !known.contains(&skin.as_str()) {
            warnings.push(format!(
                "params.skin `{skin}` is not a skin of theme `{}` (available: {})",
                theme.manifest.name,
                if known.is_empty() {
                    "none".to_string()
                } else {
                    known.join(", ")
                }
            ));
        }
    }
}

/// Warns when chat is configured but the theme never places the launcher.
fn warn_if_chat_is_unused(config: &Config, uses_chat: bool, warnings: &mut Vec<String>) {
    if config.chat.enabled() && !uses_chat {
        warnings.push(format!(
            "chat is enabled but theme `{}` never calls chat_widget(), so no chat launcher will appear. \
             Add `{{{{ chat_widget() | safe }}}}` before </body> in your base template, or run \
             `meteorite theme update {}` if this is a bundled theme",
            config.theme, config.theme
        ));
    }
}

/// Registers the template functions that do not depend on the site model: escaping, `url()`,
/// the chat widget and image processing. Returns the bundled chat widget (when it is used)
/// and the list of image files the template functions create while rendering.
fn register_core_functions(
    tera: &mut Tera,
    root: &Path,
    out_dir: &Path,
    config: &Config,
) -> (Option<crate::chat::BundledWidget>, images::Generated) {
    tera.set_escape_fn(crate::util::html_escape);
    register_url_fn(tera, &config.base_url);
    let chat_widget = (config.chat.enabled() && config.chat.widget == "bundled")
        .then(crate::chat::bundled_widget);
    crate::chat::register(tera, &config.chat, &config.base_url, chat_widget.as_ref());
    let generated: images::Generated = Arc::new(Mutex::new(Vec::new()));
    images::register(tera, root, out_dir, &config.base_url, generated.clone());
    (chat_widget, generated)
}

/// Fails early, with the list of valid names, when a highlight theme is misspelled.
fn check_highlight_themes(config: &Config) -> Result<()> {
    if !config.markdown.highlight {
        return Ok(());
    }
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
    Ok(())
}

#[derive(Serialize)]
struct MeteoriteInfo {
    version: &'static str,
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
