use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{io_err, Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub title: String,
    pub base_url: String,
    pub language: String,
    pub theme: String,
    /// Free-form values exposed to templates as `config.params`.
    pub params: BTreeMap<String, serde_norway::Value>,
    pub menus: BTreeMap<String, Vec<MenuItem>>,
    /// Taxonomy names, e.g. `[tags, categories]`.
    pub taxonomies: Vec<String>,
    /// Section path -> URL pattern using `:year :month :day :slug :section :title`.
    pub permalinks: BTreeMap<String, String>,
    /// Default page size for section listings. Unset means no pagination.
    pub paginate_by: Option<usize>,
    pub markdown: MarkdownConfig,
    pub build: BuildConfig,
    pub feed: FeedConfig,
    pub search: SearchConfig,
    pub sitemap: bool,
    /// Optional chat assistant ([PopTalk](https://github.com/pandaind/poptalk)). Off by default.
    pub chat: ChatConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MenuItem {
    pub name: String,
    pub url: String,
    pub weight: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MarkdownConfig {
    pub highlight: bool,
    /// Colour scheme used for code in light mode (and for `inline` style).
    pub highlight_theme: String,
    /// Colour scheme used for code in dark mode (`classes` style only).
    pub highlight_theme_dark: String,
    pub highlight_style: HighlightStyle,
}

/// How highlighted code is emitted.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum HighlightStyle {
    /// CSS classes plus a generated `highlight.css` with light and dark palettes.
    /// Themes must link `highlight.css`.
    #[default]
    Classes,
    /// Inline `style` attributes using `highlight_theme` only; no stylesheet needed.
    Inline,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BuildConfig {
    pub drafts: bool,
    /// Publish pages dated in the future.
    pub future: bool,
    pub minify: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FeedConfig {
    pub enabled: bool,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SearchConfig {
    pub enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            title: "My Site".into(),
            base_url: "/".into(),
            language: "en".into(),
            theme: "default".into(),
            params: BTreeMap::new(),
            menus: BTreeMap::new(),
            taxonomies: Vec::new(),
            permalinks: BTreeMap::new(),
            paginate_by: None,
            markdown: MarkdownConfig::default(),
            build: BuildConfig::default(),
            feed: FeedConfig::default(),
            search: SearchConfig::default(),
            sitemap: true,
            chat: ChatConfig::default(),
        }
    }
}

/// When the chat widget script is fetched.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ChatLoad {
    /// Show a launcher button; nothing from the chat backend is requested until it is clicked.
    #[default]
    Click,
    /// Load the widget when the browser is idle after the page has loaded.
    Idle,
    /// Load the widget with the page.
    Immediate,
}

/// Optional PopTalk integration. Everything is off unless `provider` is set.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatConfig {
    /// `poptalk` to enable the chat widget; empty (the default) disables it.
    pub provider: String,
    /// Base URL of your PopTalk backend, for example `https://chat.example.com`.
    pub api_url: String,
    /// Persona id on a multi-persona backend; empty uses the backend's default.
    pub persona: String,
    /// Widget language (`en`, `fr`, ...); empty uses the visitor's browser language.
    pub lang: String,
    /// `auto` follows the site's light/dark mode; or `dark` / `light`.
    pub theme: String,
    /// `auto` follows the site's accent colour; or any CSS colour.
    pub accent: String,
    pub load: ChatLoad,
    /// `bundled` copies the widget shipped with Meteorite into the site; or an `https://` URL.
    pub widget: String,
    /// `system` makes no request to Google Fonts; `google` loads Inter from there.
    pub fonts: String,
    /// Text of the launcher button shown before the widget is loaded.
    pub label: String,
    pub knowledge: KnowledgeConfig,
}

/// Export the site as a knowledge base for poptalk-rag.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KnowledgeConfig {
    /// Directory to write Markdown files into, usually poptalk-rag's
    /// `knowledge/<persona>/`. Relative paths are relative to the site root. Unset disables it.
    pub dir: String,
    /// Only export these sections (path prefixes such as `guide`); empty exports everything.
    pub include: Vec<String>,
    /// Never export these sections.
    pub exclude: Vec<String>,
    /// Base URL of poptalk-rag, used by `meteorite chat sync`.
    pub rag_url: String,
    /// Name of the environment variable holding the persona API key. The key itself must
    /// never be written in config.yml.
    pub key_env: String,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            provider: String::new(),
            api_url: String::new(),
            persona: String::new(),
            lang: String::new(),
            theme: "auto".into(),
            accent: "auto".into(),
            load: ChatLoad::default(),
            widget: "bundled".into(),
            fonts: "system".into(),
            label: "Chat".into(),
            knowledge: KnowledgeConfig::default(),
        }
    }
}

impl Default for KnowledgeConfig {
    fn default() -> Self {
        Self {
            dir: String::new(),
            include: Vec::new(),
            exclude: Vec::new(),
            rag_url: String::new(),
            key_env: "POPTALK_RAG_KEY".into(),
        }
    }
}

impl ChatConfig {
    pub fn enabled(&self) -> bool {
        self.provider == "poptalk"
    }

    fn validate(&self) -> Result<()> {
        let bad = |what: &str, why: &str| Error::Msg(format!("config.yml: chat.{what} {why}"));
        if !self.provider.is_empty() && self.provider != "poptalk" {
            return Err(bad(
                "provider",
                "must be `poptalk` (or empty to disable chat)",
            ));
        }
        let http = |u: &str| {
            (u.starts_with("http://") || u.starts_with("https://"))
                && !u
                    .chars()
                    .any(|c| c.is_whitespace() || c == '"' || c == '\'' || c == '<')
        };
        if self.enabled() && !http(&self.api_url) {
            return Err(bad(
                "api_url",
                "must be an http(s) URL, for example https://chat.example.com",
            ));
        }
        if !self.persona.is_empty()
            && !self
                .persona
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(bad(
                "persona",
                "may only contain letters, digits, `-` and `_`",
            ));
        }
        if !self.lang.is_empty()
            && !(self.lang.len() <= 8
                && self
                    .lang
                    .chars()
                    .all(|c| c.is_ascii_alphabetic() || c == '-'))
        {
            return Err(bad(
                "lang",
                "must be a language code such as `en` or `pt-BR`",
            ));
        }
        if !matches!(self.theme.as_str(), "auto" | "dark" | "light") {
            return Err(bad("theme", "must be `auto`, `dark` or `light`"));
        }
        if self.accent != "auto"
            && (self.accent.is_empty()
                || self.accent.len() > 64
                || self
                    .accent
                    .chars()
                    .any(|c| matches!(c, '"' | '\'' | '<' | '>' | ';' | '{' | '}' | '\\')))
        {
            return Err(bad(
                "accent",
                "must be `auto` or a plain CSS colour such as `#6c63ff`",
            ));
        }
        if self.widget != "bundled" && !self.widget.starts_with("https://") {
            return Err(bad("widget", "must be `bundled` or an https:// URL"));
        }
        if !matches!(self.fonts.as_str(), "system" | "google") {
            return Err(bad("fonts", "must be `system` or `google`"));
        }
        if self.label.chars().count() > 40 {
            return Err(bad("label", "is too long (40 characters at most)"));
        }
        let k = &self.knowledge;
        if !k.rag_url.is_empty() && !http(&k.rag_url) {
            return Err(bad("knowledge.rag_url", "must be an http(s) URL"));
        }
        if !k
            .key_env
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(bad(
                "knowledge.key_env",
                "is the NAME of an environment variable (for example POPTALK_RAG_KEY), not the key itself",
            ));
        }
        Ok(())
    }
}

impl Default for MarkdownConfig {
    fn default() -> Self {
        Self {
            highlight: true,
            highlight_theme: "InspiredGitHub".into(),
            highlight_theme_dark: "base16-ocean.dark".into(),
            highlight_style: HighlightStyle::default(),
        }
    }
}

impl Default for FeedConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            limit: 20,
        }
    }
}

impl Config {
    pub fn load(root: &Path) -> Result<Self> {
        Self::load_with_warnings(root).map(|(cfg, _)| cfg)
    }

    /// Like [`Config::load`], also returning one warning per unrecognised key (typos).
    pub fn load_with_warnings(root: &Path) -> Result<(Self, Vec<String>)> {
        let path = root.join("config.yml");
        let text = std::fs::read_to_string(&path).map_err(io_err(&path))?;
        let mut cfg: Config = if text.trim().is_empty() {
            Config::default()
        } else {
            serde_norway::from_str(&text).map_err(|e| Error::yaml(&path, &text, 0, &e))?
        };
        cfg.validate()?;
        for items in cfg.menus.values_mut() {
            items.sort_by_key(|m| m.weight);
        }
        reject_secrets(&text, &path)?;
        let warnings = unknown_keys(&text);
        Ok((cfg, warnings))
    }

    fn validate(&self) -> Result<()> {
        if self.theme.trim().is_empty() {
            return Err(Error::Msg("config.yml: `theme` must not be empty".into()));
        }
        if self.paginate_by == Some(0) {
            return Err(Error::Msg(
                "config.yml: `paginate_by` must be at least 1".into(),
            ));
        }
        self.chat.validate()?;
        for (section, pattern) in &self.permalinks {
            if !pattern.starts_with('/') {
                return Err(Error::Msg(format!(
                    "config.yml: permalink for `{section}` must start with `/` (got `{pattern}`)"
                )));
            }
        }
        Ok(())
    }
}

/// Keys of a YAML mapping, or none if it is not one.
fn keys(v: &serde_norway::Value) -> Vec<String> {
    v.as_mapping()
        .map(|m| {
            m.keys()
                .filter_map(|k| k.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Warn about keys that are not part of the configuration, with a suggestion when one is close.
///
/// The set of valid keys is read from the serialised default `Config`, so it can never drift
/// from the struct. Free-form maps (`params`, `menus`, `permalinks`) are not checked.
fn unknown_keys(text: &str) -> Vec<String> {
    let Ok(user) = serde_norway::from_str::<serde_norway::Value>(text) else {
        return Vec::new();
    };
    let Ok(known) = serde_norway::to_value(Config::default()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut check = |scope: &str, user_keys: Vec<String>, known_keys: Vec<String>| {
        for k in user_keys {
            if known_keys.contains(&k) {
                continue;
            }
            let at = if scope.is_empty() {
                k.clone()
            } else {
                format!("{scope}.{k}")
            };
            let hint = known_keys
                .iter()
                .filter(|c| is_close(&k, c))
                .min_by_key(|c| edit_distance(&k, c))
                .map(|c| {
                    let full = if scope.is_empty() {
                        c.to_string()
                    } else {
                        format!("{scope}.{c}")
                    };
                    format!(" (did you mean `{full}`?)")
                })
                .unwrap_or_default();
            out.push(format!(
                "config.yml: unknown key `{at}`{hint}; it has no effect"
            ));
        }
    };
    check("", keys(&user), keys(&known));
    for section in ["markdown", "build", "feed", "search"] {
        if let (Some(u), Some(k)) = (user.get(section), known.get(section)) {
            check(section, keys(u), keys(k));
        }
    }
    out
}

fn is_close(a: &str, b: &str) -> bool {
    let allowed = 2.max(a.chars().count().max(b.chars().count()) / 3);
    edit_distance(a, b) <= allowed || a.starts_with(b) || b.starts_with(a)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            cur.push(sub.min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Keys under `chat:` that look like credentials. config.yml is committed and published with the
/// site, so secrets belong in the environment, never here.
fn reject_secrets(text: &str, path: &Path) -> Result<()> {
    let Ok(v) = serde_norway::from_str::<serde_norway::Value>(text) else {
        return Ok(());
    };
    fn walk(v: &serde_norway::Value, trail: &str, out: &mut Vec<String>) {
        if let Some(m) = v.as_mapping() {
            for (k, val) in m {
                let Some(k) = k.as_str() else { continue };
                let at = format!("{trail}.{k}");
                let lower = k.to_ascii_lowercase();
                let secretish = [
                    "secret", "password", "token", "apikey", "api_key", "api-key", "private",
                ]
                .iter()
                .any(|s| lower.contains(s))
                    || lower == "key";
                // `key_env` is a variable *name*, which is exactly what we want.
                if secretish && lower != "key_env" {
                    out.push(at.clone());
                }
                walk(val, &at, out);
            }
        }
    }
    let mut found = Vec::new();
    if let Some(chat) = v.get("chat") {
        walk(chat, "chat", &mut found);
    }
    if found.is_empty() {
        return Ok(());
    }
    Err(Error::content(
        path,
        format!(
            "{} looks like a credential. config.yml is published with your site; put secrets in an \
             environment variable and name it with `chat.knowledge.key_env`",
            found.join(", ")
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warns_about_typos_with_suggestions() {
        let w = unknown_keys(
            "title: x\nbaseurl: y\ntaxonomy: [tags]\nmarkdown:\n  hilight: true\nbuild:\n  minfy: true\n",
        );
        let joined = w.join("\n");
        assert!(
            joined.contains("unknown key `baseurl` (did you mean `base_url`?)"),
            "{joined}"
        );
        assert!(
            joined.contains("`taxonomy` (did you mean `taxonomies`?)"),
            "{joined}"
        );
        assert!(
            joined.contains("`markdown.hilight` (did you mean `markdown.highlight`?)"),
            "{joined}"
        );
        assert!(
            joined.contains("`build.minfy` (did you mean `build.minify`?)"),
            "{joined}"
        );
    }

    #[test]
    fn valid_and_free_form_keys_are_silent() {
        let ok = "title: x\nbase_url: /\ntheme: default\nparams:\n  anything: 1\n  at_all: {a: b}\nmenus:\n  main: []\npermalinks:\n  blog: /x/\nmarkdown:\n  highlight: true\n  highlight_style: classes\npaginate_by: 5\n";
        assert!(unknown_keys(ok).is_empty(), "{:?}", unknown_keys(ok));
        // every key of the default config is accepted
        let all = serde_norway::to_string(&Config::default()).unwrap();
        assert!(unknown_keys(&all).is_empty());
    }

    #[test]
    fn unrelated_unknown_key_has_no_suggestion() {
        let w = unknown_keys("zzzzzzzz: 1\n");
        assert_eq!(
            w,
            vec!["config.yml: unknown key `zzzzzzzz`; it has no effect".to_string()]
        );
    }
}
