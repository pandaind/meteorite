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
        }
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
