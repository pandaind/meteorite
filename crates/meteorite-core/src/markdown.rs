//! Markdown -> HTML with heading anchors, a table of contents and code highlighting.

use std::collections::HashMap;
use std::sync::OnceLock;

use pulldown_cmark::{
    html, CodeBlockKind, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};
use serde::Serialize;
use syntect::highlighting::{Color, Theme, ThemeSet};
use syntect::html::{
    css_for_theme_with_class_style, highlighted_html_for_string, ClassStyle, ClassedHTMLGenerator,
};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::util::slugify;

#[derive(Debug, Clone)]
pub struct MarkdownOptions {
    pub highlight: bool,
    pub highlight_theme: String,
    /// Emit CSS classes (see [`highlight_css`]) instead of inline styles.
    pub classed: bool,
}

const CLASS_PREFIX: &str = "hl-";

#[derive(Debug, Clone, Serialize)]
pub struct TocEntry {
    pub level: u8,
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct Rendered {
    pub html: String,
    pub toc: Vec<TocEntry>,
}

fn parser_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_SMART_PUNCTUATION
        | Options::ENABLE_HEADING_ATTRIBUTES
}

pub fn render(body: &str, opts: &MarkdownOptions) -> Rendered {
    let events: Vec<Event> = Parser::new_ext(body, parser_options()).collect();
    let mut out: Vec<Event> = Vec::with_capacity(events.len());
    let mut toc = Vec::new();
    let mut used: HashMap<String, usize> = HashMap::new();

    let mut i = 0;
    while i < events.len() {
        match &events[i] {
            Event::Start(Tag::Heading {
                level,
                id,
                classes,
                attrs,
            }) => {
                let mut j = i + 1;
                let mut text = String::new();
                while j < events.len() && !matches!(events[j], Event::End(TagEnd::Heading(_))) {
                    if let Event::Text(t) | Event::Code(t) = &events[j] {
                        text.push_str(t);
                    }
                    j += 1;
                }
                let id_str = match id {
                    Some(explicit) => explicit.to_string(),
                    None => unique_id(&text, &mut used),
                };
                toc.push(TocEntry {
                    level: heading_level(*level),
                    id: id_str.clone(),
                    title: text,
                });
                out.push(Event::Start(Tag::Heading {
                    level: *level,
                    id: Some(CowStr::from(id_str.clone())),
                    classes: classes.clone(),
                    attrs: attrs.clone(),
                }));
                out.extend(events[i + 1..j.min(events.len())].iter().cloned());
                out.push(Event::Html(CowStr::from(format!(
                    " <a class=\"anchor\" href=\"#{id_str}\" aria-label=\"Link to this section\">#</a>"
                ))));
                if j < events.len() {
                    out.push(events[j].clone());
                }
                i = j + 1;
            }
            Event::Start(Tag::CodeBlock(kind)) if opts.highlight => {
                let mut j = i + 1;
                let mut code = String::new();
                while j < events.len() && !matches!(events[j], Event::End(TagEnd::CodeBlock)) {
                    if let Event::Text(t) = &events[j] {
                        code.push_str(t);
                    }
                    j += 1;
                }
                let highlighted = match kind {
                    CodeBlockKind::Fenced(info) => {
                        let lang = info
                            .split(|c: char| c == ',' || c == '{' || c.is_whitespace())
                            .next()
                            .unwrap_or("");
                        if lang.is_empty() {
                            None
                        } else {
                            highlight(&code, lang, opts)
                        }
                    }
                    CodeBlockKind::Indented => None,
                };
                match highlighted {
                    Some(h) => out.push(Event::Html(CowStr::from(h))),
                    None => out.extend(events[i..(j + 1).min(events.len())].iter().cloned()),
                }
                i = j + 1;
            }
            _ => {
                out.push(events[i].clone());
                i += 1;
            }
        }
    }

    let mut html = String::new();
    html::push_html(&mut html, out.into_iter());
    Rendered { html, toc }
}

fn heading_level(l: HeadingLevel) -> u8 {
    l as usize as u8
}

fn unique_id(text: &str, used: &mut HashMap<String, usize>) -> String {
    let base = match slugify(text) {
        s if s.is_empty() => "section".to_string(),
        s => s,
    };
    let n = used.entry(base.clone()).or_insert(0);
    let id = if *n == 0 { base } else { format!("{base}-{n}") };
    *n += 1;
    id
}

static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
static THEMES: OnceLock<ThemeSet> = OnceLock::new();

fn class_style() -> ClassStyle {
    ClassStyle::SpacedPrefixed {
        prefix: CLASS_PREFIX,
    }
}

fn highlight(code: &str, lang: &str, opts: &MarkdownOptions) -> Option<String> {
    let ss = SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines);
    let syntax = ss.find_syntax_by_token(lang)?;
    if opts.classed {
        let mut gen = ClassedHTMLGenerator::new_with_class_style(syntax, ss, class_style());
        for line in LinesWithEndings::from(code) {
            gen.parse_html_for_line_which_includes_newline(line).ok()?;
        }
        let lang_class = crate::util::slugify(lang);
        return Some(format!(
            "<pre class=\"hl\"><code class=\"language-{lang_class}\">{}</code></pre>\n",
            gen.finalize()
        ));
    }
    let ts = THEMES.get_or_init(ThemeSet::load_defaults);
    let theme = ts
        .themes
        .get(&opts.highlight_theme)
        .or_else(|| ts.themes.values().next())?;
    highlighted_html_for_string(code, ss, syntax, theme).ok()
}

fn hex(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// CSS for one theme: the token rules plus the `pre.hl` background and text colour.
fn theme_css(theme: &Theme) -> String {
    let mut css = String::new();
    let mut pre = String::new();
    if let Some(bg) = theme.settings.background {
        pre.push_str(&format!("  background-color: {};\n", hex(bg)));
    }
    if let Some(fg) = theme.settings.foreground {
        pre.push_str(&format!("  color: {};\n", hex(fg)));
    }
    if !pre.is_empty() {
        css.push_str(&format!("pre.hl {{\n{pre}}}\n"));
    }
    if let Ok(tokens) = css_for_theme_with_class_style(theme, class_style()) {
        css.push_str(&tokens);
    }
    css
}

/// Prefix every selector in flat (non-nested) CSS with `scope`.
fn scope_css(css: &str, scope: &str) -> String {
    let mut out = String::with_capacity(css.len() + 256);
    for line in css.lines() {
        match line.strip_suffix('{').or_else(|| line.strip_suffix("{ ")) {
            Some(selectors) if !selectors.trim().is_empty() => {
                let scoped: Vec<String> = selectors
                    .split(',')
                    .map(|s| format!("{scope} {}", s.trim()))
                    .collect();
                out.push_str(&scoped.join(", "));
                out.push_str(" {\n");
            }
            _ => {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out
}

/// The stylesheet for `classed` highlighting: the light theme as the default, the dark
/// theme under `prefers-color-scheme: dark` (unless the page pins `data-theme="light"`)
/// and under an explicit `data-theme="dark"`.
pub fn highlight_css(light: &str, dark: &str) -> String {
    let ts = THEMES.get_or_init(ThemeSet::load_defaults);
    let pick = |name: &str| ts.themes.get(name).or_else(|| ts.themes.values().next());
    let (Some(l), Some(d)) = (pick(light), pick(dark)) else {
        return String::new();
    };
    let light_css = theme_css(l);
    let dark_css = theme_css(d);
    let mut out = String::from("/* Generated by Meteorite: code highlighting. */\n");
    out.push_str(&light_css);
    out.push_str("@media (prefers-color-scheme: dark) {\n");
    out.push_str(&indent(&scope_css(
        &dark_css,
        ":root:not([data-theme=\"light\"])",
    )));
    out.push_str("}\n");
    out.push_str(&scope_css(&dark_css, ":root[data-theme=\"dark\"]"));
    out
}

fn indent(s: &str) -> String {
    s.lines().map(|l| format!("  {l}\n")).collect()
}

/// Whether `name` is one of the bundled highlight themes.
pub fn is_highlight_theme(name: &str) -> bool {
    THEMES
        .get_or_init(ThemeSet::load_defaults)
        .themes
        .contains_key(name)
}

/// Names of the bundled highlight themes, for error messages and docs.
pub fn highlight_theme_names() -> Vec<String> {
    THEMES
        .get_or_init(ThemeSet::load_defaults)
        .themes
        .keys()
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(highlight: bool) -> MarkdownOptions {
        MarkdownOptions {
            classed: false,
            highlight,
            highlight_theme: "InspiredGitHub".into(),
        }
    }

    #[test]
    fn headings_get_unique_ids_and_toc() {
        let r = render("# Intro\n\n## Intro\n\n### `Code` bit\n", &opts(false));
        let ids: Vec<_> = r.toc.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["intro", "intro-1", "code-bit"]);
        assert_eq!(r.toc[2].level, 3);
        assert!(r.html.contains("<h1 id=\"intro\">Intro"));
        assert!(r.html.contains("href=\"#intro-1\""));
    }

    #[test]
    fn highlighting_applies_to_known_languages_only() {
        let r = render(
            "```rust\nfn main() {}\n```\n\n```nosuchlang\nx\n```\n",
            &opts(true),
        );
        assert!(
            r.html.contains("style=\""),
            "rust block should be highlighted"
        );
        assert!(r.html.contains("<pre><code class=\"language-nosuchlang\">"));
    }

    #[test]
    fn classed_highlighting_has_no_inline_styles() {
        let o = MarkdownOptions {
            classed: true,
            ..opts(true)
        };
        let r = render("```rust\nfn main() {}\n```\n", &o);
        assert!(!r.html.contains("style="), "{}", r.html);
        assert!(r
            .html
            .contains("<pre class=\"hl\"><code class=\"language-rust\">"));
        assert!(r.html.contains("class=\"hl-"), "{}", r.html);
    }

    #[test]
    fn highlight_css_has_light_and_dark_palettes() {
        let css = highlight_css("InspiredGitHub", "base16-ocean.dark");
        assert!(css.contains("pre.hl {"));
        assert!(css.contains("@media (prefers-color-scheme: dark)"));
        assert!(css.contains(":root[data-theme=\"dark\"] pre.hl"));
        assert!(css.contains(":root:not([data-theme=\"light\"]) pre.hl"));
        assert!(css.contains(":root[data-theme=\"dark\"] .hl-"), "{css}");
        assert!(is_highlight_theme("base16-ocean.dark") && !is_highlight_theme("nope"));
    }

    #[test]
    fn highlighting_can_be_disabled() {
        let r = render("```rust\nfn main() {}\n```\n", &opts(false));
        assert!(r.html.contains("<code class=\"language-rust\">"));
    }
}
