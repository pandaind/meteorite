//! Markdown -> HTML with heading anchors, a table of contents and code highlighting.

use std::collections::HashMap;
use std::sync::OnceLock;

use pulldown_cmark::{
    html, CodeBlockKind, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};
use serde::Serialize;
use syntect::highlighting::ThemeSet;
use syntect::html::highlighted_html_for_string;
use syntect::parsing::SyntaxSet;

use crate::util::slugify;

#[derive(Debug, Clone)]
pub struct MarkdownOptions {
    pub highlight: bool,
    pub highlight_theme: String,
}

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
                            highlight(&code, lang, &opts.highlight_theme)
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

fn highlight(code: &str, lang: &str, theme_name: &str) -> Option<String> {
    let ss = SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines);
    let syntax = ss.find_syntax_by_token(lang)?;
    let ts = THEMES.get_or_init(ThemeSet::load_defaults);
    let theme = ts
        .themes
        .get(theme_name)
        .or_else(|| ts.themes.values().next())?;
    highlighted_html_for_string(code, ss, syntax, theme).ok()
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
    fn highlighting_can_be_disabled() {
        let r = render("```rust\nfn main() {}\n```\n", &opts(false));
        assert!(r.html.contains("<code class=\"language-rust\">"));
    }
}
