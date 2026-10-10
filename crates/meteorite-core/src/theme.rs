//! Theme manifest, validation and installation.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::bundled;
use crate::error::{io_err, Error, Result};

/// Version of the template context and file conventions themes are written against.
pub const API_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeManifest {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default = "default_api")]
    pub meteorite_api: u32,
    #[serde(default)]
    pub params: BTreeMap<String, ParamSpec>,
}

fn default_api() -> u32 {
    API_VERSION
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ParamSpec {
    pub default: Option<serde_norway::Value>,
    pub required: bool,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub dir: PathBuf,
    pub manifest: ThemeManifest,
}

impl Theme {
    pub fn load(dir: &Path) -> Result<Theme> {
        let dirname = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !dir.is_dir() {
            return Err(Error::ThemeNotFound(dirname));
        }
        let path = dir.join("theme.yml");
        let text = fs::read_to_string(&path).map_err(|_| Error::ThemeInvalid {
            theme: dirname.clone(),
            reason: "missing theme.yml".into(),
        })?;
        let manifest: ThemeManifest =
            serde_norway::from_str(&text).map_err(|e| Error::yaml(&path, &text, 0, &e))?;
        if manifest.meteorite_api != API_VERSION {
            return Err(Error::ThemeInvalid {
                theme: manifest.name.clone(),
                reason: format!(
                    "targets theme API v{} but this Meteorite supports v{API_VERSION}",
                    manifest.meteorite_api
                ),
            });
        }
        check_name(&manifest.name)?;
        if !dir.join("templates").is_dir() {
            return Err(Error::ThemeInvalid {
                theme: manifest.name,
                reason: "missing templates/ directory".into(),
            });
        }
        Ok(Theme {
            dir: dir.to_path_buf(),
            manifest,
        })
    }

    /// Fill unset site params from theme defaults; fail on missing required ones.
    pub fn resolve_params(&self, params: &mut BTreeMap<String, serde_norway::Value>) -> Result<()> {
        let mut missing = Vec::new();
        for (key, spec) in &self.manifest.params {
            if params.contains_key(key) {
                continue;
            }
            match &spec.default {
                Some(v) => {
                    params.insert(key.clone(), v.clone());
                }
                None if spec.required => missing.push(key.as_str()),
                None => {}
            }
        }
        if missing.is_empty() {
            Ok(())
        } else {
            Err(Error::MissingThemeParams {
                theme: self.manifest.name.clone(),
                params: missing.join(", "),
            })
        }
    }
}

/// Theme names become directory names, so keep them to a safe alphabet.
pub fn check_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok {
        Ok(())
    } else {
        Err(Error::InvalidPath(format!(
            "theme name `{name}` (use letters, digits, `-` and `_` only)"
        )))
    }
}

pub struct Installed {
    pub dir_name: String,
    pub manifest: Option<ThemeManifest>,
    pub error: Option<String>,
}

/// Themes found under `themes/`, valid or not.
pub fn list_installed(root: &Path) -> Result<Vec<Installed>> {
    let dir = root.join("themes");
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    let mut names: Vec<_> = fs::read_dir(&dir)
        .map_err(io_err(&dir))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    for dir_name in names {
        match Theme::load(&dir.join(&dir_name)) {
            Ok(t) => out.push(Installed {
                dir_name,
                manifest: Some(t.manifest),
                error: None,
            }),
            Err(e) => out.push(Installed {
                dir_name,
                manifest: None,
                error: Some(e.to_string()),
            }),
        }
    }
    Ok(out)
}

/// Install a theme from a bundled name, a local directory, or a git URL.
/// Returns the directory name it was installed under.
pub fn add(root: &Path, source: &str, name: Option<&str>) -> Result<String> {
    if let Some(n) = name {
        check_name(n)?;
    }
    let themes = root.join("themes");
    fs::create_dir_all(&themes).map_err(io_err(&themes))?;
    let staging = themes.join(format!(".staging-{}", std::process::id()));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(io_err(&staging))?;
    }

    let result = (|| -> Result<String> {
        let local = Path::new(source);
        if local.is_dir() {
            copy_tree(local, &staging)?;
        } else if let Some(b) = bundled::find(source) {
            b.write_to(&staging)?;
        } else if is_git_url(source) {
            git_clone(source, &staging)?;
        } else {
            return Err(Error::Msg(format!(
                "`{source}` is not a bundled theme ({}), a directory, or a git URL",
                bundled::names().join(", ")
            )));
        }
        let theme = Theme::load(&staging)?;
        let final_name = name
            .map(str::to_string)
            .unwrap_or_else(|| theme.manifest.name.clone());
        check_name(&final_name)?;
        let dest = themes.join(&final_name);
        if dest.exists() {
            return Err(Error::AlreadyExists(dest));
        }
        fs::rename(&staging, &dest).map_err(io_err(&dest))?;
        Ok(final_name)
    })();

    if staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

pub fn remove(root: &Path, name: &str, active: &str) -> Result<()> {
    check_name(name)?;
    if name == active {
        return Err(Error::Msg(format!(
            "`{name}` is the active theme; change `theme:` in config.yml first"
        )));
    }
    let dir = root.join("themes").join(name);
    if !dir.is_dir() {
        return Err(Error::ThemeNotFound(name.to_string()));
    }
    fs::remove_dir_all(&dir).map_err(io_err(&dir))
}

fn is_git_url(s: &str) -> bool {
    s.contains("://") || s.starts_with("git@") || s.ends_with(".git")
}

fn git_clone(url: &str, dest: &Path) -> Result<()> {
    // `--` stops a URL like `--upload-pack=...` from being parsed as an option.
    // Restrict transports: `ext::` would let a URL run arbitrary commands.
    let status = Command::new("git")
        .env("GIT_ALLOW_PROTOCOL", "https:ssh:git:file")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args([
            "clone",
            "--depth",
            "1",
            "--quiet",
            "--no-recurse-submodules",
            "--",
        ])
        .arg(url)
        .arg(dest)
        .status()
        .map_err(|e| Error::Msg(format!("could not run git: {e}")))?;
    if !status.success() {
        return Err(Error::Msg(format!("git clone failed for {url}")));
    }
    let git_dir = dest.join(".git");
    if git_dir.exists() {
        fs::remove_dir_all(&git_dir).map_err(io_err(&git_dir))?;
    }
    remove_symlinks(dest)
}

/// A theme must not be able to point at files outside itself.
fn remove_symlinks(dir: &Path) -> Result<()> {
    for entry in WalkDir::new(dir).follow_links(false) {
        let entry = entry.map_err(|e| Error::Io {
            path: dir.to_path_buf(),
            source: e.into(),
        })?;
        if entry.path_is_symlink() {
            fs::remove_file(entry.path()).map_err(io_err(entry.path()))?;
        }
    }
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    for entry in WalkDir::new(from).follow_links(false) {
        let entry = entry.map_err(|e| Error::Io {
            path: from.to_path_buf(),
            source: e.into(),
        })?;
        let rel = entry.path().strip_prefix(from).unwrap();
        if rel.components().any(|c| c.as_os_str() == ".git") {
            continue;
        }
        let dest = to.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&dest).map_err(io_err(&dest))?;
        } else if entry.file_type().is_file() {
            fs::copy(entry.path(), &dest).map_err(io_err(&dest))?;
        }
        // Symlinks are skipped on purpose: a theme must not smuggle in links to elsewhere.
    }
    Ok(())
}
