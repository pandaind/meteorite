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
    pub highlight_theme: String,
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
        Ok(cfg)
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
