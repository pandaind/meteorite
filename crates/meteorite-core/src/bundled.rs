//! Themes compiled into the binary, so `meteorite new site` and
//! `meteorite theme add <name>` work offline.

use std::fs;
use std::path::Path;

use crate::error::{io_err, Result};

pub struct Bundled {
    pub name: &'static str,
    /// `(path relative to the theme dir, contents)`
    pub files: &'static [(&'static str, &'static str)],
}

impl Bundled {
    pub fn write_to(&self, dir: &Path) -> Result<()> {
        for (rel, body) in self.files {
            let path = dir.join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(io_err(parent))?;
            }
            fs::write(&path, body).map_err(io_err(&path))?;
        }
        Ok(())
    }
}

static DEFAULT: Bundled = Bundled {
    name: "default",
    files: &[
        (
            "static/style.css",
            include_str!("../assets/themes/default/static/style.css"),
        ),
        (
            "templates/404.html",
            include_str!("../assets/themes/default/templates/404.html"),
        ),
        (
            "templates/base.html",
            include_str!("../assets/themes/default/templates/base.html"),
        ),
        (
            "templates/index.html",
            include_str!("../assets/themes/default/templates/index.html"),
        ),
        (
            "templates/page.html",
            include_str!("../assets/themes/default/templates/page.html"),
        ),
        (
            "templates/section.html",
            include_str!("../assets/themes/default/templates/section.html"),
        ),
        (
            "templates/shortcodes/note.html",
            include_str!("../assets/themes/default/templates/shortcodes/note.html"),
        ),
        (
            "templates/shortcodes/youtube.html",
            include_str!("../assets/themes/default/templates/shortcodes/youtube.html"),
        ),
        (
            "templates/taxonomy.html",
            include_str!("../assets/themes/default/templates/taxonomy.html"),
        ),
        (
            "templates/term.html",
            include_str!("../assets/themes/default/templates/term.html"),
        ),
        (
            "theme.yml",
            include_str!("../assets/themes/default/theme.yml"),
        ),
    ],
};

static TERMINAL: Bundled = Bundled {
    name: "terminal",
    files: &[
        (
            "static/terminal.css",
            include_str!("../assets/themes/terminal/static/terminal.css"),
        ),
        (
            "templates/404.html",
            include_str!("../assets/themes/terminal/templates/404.html"),
        ),
        (
            "templates/base.html",
            include_str!("../assets/themes/terminal/templates/base.html"),
        ),
        (
            "templates/index.html",
            include_str!("../assets/themes/terminal/templates/index.html"),
        ),
        (
            "templates/page.html",
            include_str!("../assets/themes/terminal/templates/page.html"),
        ),
        (
            "templates/section.html",
            include_str!("../assets/themes/terminal/templates/section.html"),
        ),
        (
            "templates/shortcodes/note.html",
            include_str!("../assets/themes/terminal/templates/shortcodes/note.html"),
        ),
        (
            "templates/shortcodes/youtube.html",
            include_str!("../assets/themes/terminal/templates/shortcodes/youtube.html"),
        ),
        (
            "templates/taxonomy.html",
            include_str!("../assets/themes/terminal/templates/taxonomy.html"),
        ),
        (
            "templates/term.html",
            include_str!("../assets/themes/terminal/templates/term.html"),
        ),
        (
            "theme.yml",
            include_str!("../assets/themes/terminal/theme.yml"),
        ),
    ],
};

static ALL: &[&Bundled] = &[&DEFAULT, &TERMINAL];

pub fn find(name: &str) -> Option<&'static Bundled> {
    ALL.iter().copied().find(|b| b.name == name)
}

pub fn names() -> Vec<&'static str> {
    ALL.iter().map(|b| b.name).collect()
}
