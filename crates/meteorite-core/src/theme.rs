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
    /// Named palettes the theme can switch between without changing its markup.
    #[serde(default)]
    pub skins: Vec<SkinSpec>,
}

/// A skin: a named look inside one theme. The theme applies it with a `data-skin` attribute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkinSpec {
    pub name: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub description: String,
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
        let mut seen = std::collections::BTreeSet::new();
        for skin in &manifest.skins {
            if check_name(&skin.name).is_err() || skin.name != skin.name.to_ascii_lowercase() {
                return Err(Error::ThemeInvalid {
                    theme: manifest.name.clone(),
                    reason: format!(
                        "skin name `{}` must be lowercase letters, digits, `-` or `_`",
                        skin.name
                    ),
                });
            }
            if !seen.insert(skin.name.clone()) {
                return Err(Error::ThemeInvalid {
                    theme: manifest.name.clone(),
                    reason: format!("skin `{}` is declared twice", skin.name),
                });
            }
        }
        if !dir.join("templates").is_dir() {
            return Err(Error::ThemeInvalid {
                theme: manifest.name,
                reason: "missing templates/ directory".into(),
            });
        }
        let mut manifest = manifest;
        for skin in &mut manifest.skins {
            if skin.label.is_empty() {
                skin.label = crate::util::humanize(&skin.name);
            }
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

/// Compare dotted numeric versions (`0.4.0` > `0.3.9`). Unparseable parts count as 0.
pub fn version_newer(candidate: &str, installed: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.split('.')
            .map(|p| {
                p.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0)
            })
            .collect()
    };
    let (a, b) = (parse(candidate), parse(installed));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        if x != y {
            return x > y;
        }
    }
    false
}

/// If `name` is a bundled theme whose bundled version is newer than `installed_version`,
/// return that newer version.
pub fn update_available(name: &str, installed_version: &str) -> Option<String> {
    let b = bundled::find(name)?;
    let v = b.version();
    version_newer(&v, installed_version).then_some(v)
}

pub struct Updated {
    pub name: String,
    pub from: String,
    pub to: String,
    /// Where the previous copy was moved; nothing is deleted.
    pub backup: PathBuf,
}

/// Replace an installed copy of a *bundled* theme with the version built into this binary.
///
/// The old copy is moved to `themes/.backup/<name>-<version>-<timestamp>/` first, so local
/// edits are never lost. Customisations kept in the site's own `templates/` and `static/`
/// are not touched.
pub fn update(root: &Path, name: &str) -> Result<Updated> {
    check_name(name)?;
    let bundled = bundled::find(name).ok_or_else(|| {
        Error::Msg(format!(
            "`{name}` is not a bundled theme ({}); reinstall third-party themes with `theme remove` and `theme add`",
            bundled::names().join(", ")
        ))
    })?;
    let themes = root.join("themes");
    let dir = themes.join(name);
    if !dir.is_dir() {
        return Err(Error::ThemeNotFound(name.to_string()));
    }
    let from = Theme::load(&dir)
        .map(|t| t.manifest.version)
        .unwrap_or_else(|_| "unknown".to_string());
    let to = bundled.version();

    let staging = themes.join(format!(".staging-{}", std::process::id()));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(io_err(&staging))?;
    }
    bundled.write_to(&staging)?;
    Theme::load(&staging)?; // never swap in something that doesn't load

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backups = themes.join(".backup");
    fs::create_dir_all(&backups).map_err(io_err(&backups))?;
    let backup = backups.join(format!("{name}-{from}-{stamp}"));
    fs::rename(&dir, &backup).map_err(io_err(&backup))?;
    if let Err(e) = fs::rename(&staging, &dir) {
        let _ = fs::rename(&backup, &dir); // put the old copy back
        let _ = fs::remove_dir_all(&staging);
        return Err(io_err(&dir)(e));
    }
    Ok(Updated {
        name: name.to_string(),
        from,
        to,
        backup,
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn version_ordering() {
        assert!(version_newer("0.4.0", "0.3.0"));
        assert!(version_newer("0.10.0", "0.9.9"));
        assert!(version_newer("1.0", "0.9.9"));
        assert!(!version_newer("0.3.0", "0.3.0"));
        assert!(!version_newer("0.3.0", "0.4.0"));
        assert!(!version_newer("", "0.1.0"));
    }

    fn theme_dir(name: &str, yml: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("meteorite-theme-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("templates")).unwrap();
        fs::write(d.join("theme.yml"), yml).unwrap();
        d
    }

    #[test]
    fn skins_are_parsed_and_labelled() {
        let d = theme_dir(
            "ok",
            "name: t\nskins:\n  - { name: paper, label: Paper }\n  - { name: deep-blue }\n",
        );
        let t = Theme::load(&d).unwrap();
        assert_eq!(t.manifest.skins[0].label, "Paper");
        assert_eq!(t.manifest.skins[1].label, "Deep blue");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn bad_skins_are_rejected() {
        for (n, yml) in [
            ("dup", "name: t\nskins:\n  - { name: a }\n  - { name: a }\n"),
            ("caps", "name: t\nskins:\n  - { name: Paper }\n"),
            ("path", "name: t\nskins:\n  - { name: \"../x\" }\n"),
        ] {
            let d = theme_dir(n, yml);
            assert!(Theme::load(&d).is_err(), "{n}");
            let _ = fs::remove_dir_all(d);
        }
    }
}
