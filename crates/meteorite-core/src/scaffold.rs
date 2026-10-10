use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::bundled;
use crate::error::{io_err, Error, Result};
use crate::util::humanize;

fn config_yml(theme: &str) -> String {
    format!(
        "\
# Site configuration. Docs: https://github.com/pandaind/meteorite
title: My Meteorite Site
# Set this to your production URL. `meteorite serve` overrides it locally.
base_url: https://example.com/
language: en
theme: {theme}

params:
  author: Me
  # Theme options (see the theme's theme.yml for the full list):
  # color_scheme: dark          # dark | light | auto (follow the OS)
  # github_url: https://github.com/you/your-repo
  # header_cta_text: Subscribe
  # header_cta_url: /blog/

menus:
  main:
    - {{ name: Home, url: /, weight: 1 }}
    - {{ name: Blog, url: /blog/, weight: 2 }}
    - {{ name: About, url: /about/, weight: 3 }}

taxonomies: [tags]
paginate_by: 10

markdown:
  highlight: true
  highlight_theme: InspiredGitHub          # code colours in light mode
  highlight_theme_dark: base16-ocean.dark  # code colours in dark mode

build:
  drafts: false
  minify: false
"
    )
}

const HOME: &str = r#"---
title: Welcome
description: A fast static site, built with Meteorite.
hero:
  eyebrow: Built with Meteorite
  title: Your site, written in plain Markdown
  subtitle: "Content in Markdown, configuration in YAML, themes you can swap in one line. No database, no build chain."
  command: meteorite serve
  actions:
    - { name: Read the blog, url: /blog/, style: primary, icon: book }
    - { name: About, url: /about/, style: secondary }
stats:
  - { value: Markdown, label: "your content, in plain files" }
  - { value: YAML, label: one config file }
  - { value: "0", label: databases to run }
features:
  title: Everything in one folder
  subtitle: "Edit these blocks in content/_index.md. Each one is optional."
  items:
    - { icon: folder, title: Plain files, text: "Pages are Markdown with YAML front matter. Keep the whole site in git." }
    - { icon: palette, title: Themes you can swap, text: "Change one line in config.yml and the same content gets a new look." }
    - { icon: bolt, title: Fast to build, text: "Rebuilds while you type, with live reload in the browser." }
    - { icon: code, title: Code that looks right, text: "Syntax highlighting with light and dark palettes, plus copy buttons." }
    - { icon: search, title: Search and feeds, text: "A search index, Atom and RSS feeds and a sitemap, generated for you." }
    - { icon: shield, title: Safe by default, text: "A built-in link checker catches broken pages before your readers do." }
latest:
  title: From the blog
  section: blog
  limit: 3
cta:
  title: Make it yours
  text: "Replace this page, add posts under content/blog, and set your own title and URL in config.yml."
  actions:
    - { name: Read the blog, url: /blog/, style: primary }
---
"#;

const ABOUT: &str = "\
---
title: About
description: About this site
---
Pages are Markdown files. Their URL follows the file path, so this one lives at `/about/`.
";

const BLOG_INDEX: &str = "\
---
title: Blog
description: Posts
sort_by: date
paginate_by: 5
---
Everything I've written.
";

fn hello_post(today: &str) -> String {
    format!(
        "\
---
title: Hello, world
date: {today}
tags: [meteorite, intro]
description: The first post.
---
Welcome! This post shows a few features.

<!-- more -->

## Code

```rust
fn main() {{
    println!(\"hello from Meteorite\");
}}
```

## Shortcodes

{{{{< note >}}}}
Shortcodes are small reusable templates from `templates/shortcodes/`.
{{{{< /note >}}}}
"
    )
}

pub fn new_site(dir: &Path, theme: &str) -> Result<()> {
    let bundled = bundled::find(theme).ok_or_else(|| {
        Error::Msg(format!(
            "unknown theme `{theme}` (bundled: {})",
            bundled::names().join(", ")
        ))
    })?;
    if dir.join("config.yml").exists() {
        return Err(Error::AlreadyExists(dir.join("config.yml")));
    }
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    for (rel, body) in [
        ("config.yml", config_yml(theme)),
        ("content/_index.md", HOME.to_string()),
        ("content/about.md", ABOUT.to_string()),
        ("content/blog/_index.md", BLOG_INDEX.to_string()),
        ("content/blog/hello-world.md", hello_post(&today)),
        (".gitignore", "public/\n".to_string()),
    ] {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io_err(parent))?;
        }
        fs::write(&path, body).map_err(io_err(&path))?;
    }
    for sub in ["static", "data", "templates"] {
        let p = dir.join(sub);
        fs::create_dir_all(&p).map_err(io_err(&p))?;
    }
    bundled.write_to(&dir.join("themes").join(theme))
}

/// Create `content/<rel>` with starter front matter. Returns the file path.
pub fn new_page(root: &Path, rel: &str, title: Option<&str>) -> Result<PathBuf> {
    let mut rel_path = PathBuf::from(rel);
    if rel_path.extension().is_none() {
        rel_path.set_extension("md");
    }
    if rel_path.extension().is_some_and(|e| e != "md") {
        return Err(Error::InvalidPath(format!(
            "{rel} (content files must end in .md)"
        )));
    }
    if rel_path.is_absolute()
        || rel_path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(Error::InvalidPath(format!(
            "{rel} (must be relative to content/, without `..`)"
        )));
    }
    let content = root.join("content");
    if !content.is_dir() {
        return Err(Error::Msg(format!(
            "{} not found; run this inside a Meteorite site",
            content.display()
        )));
    }
    let path = content.join(&rel_path);
    if path.exists() {
        return Err(Error::AlreadyExists(path));
    }
    let stem = rel_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled");
    let title = title.map(str::to_string).unwrap_or_else(|| {
        humanize(if stem == "_index" || stem == "index" {
            rel_path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or(stem)
        } else {
            stem
        })
    });
    let escaped = title.replace('\\', "\\\\").replace('"', "\\\"");
    let today = chrono::Utc::now().format("%Y-%m-%d");
    let body = format!("---\ntitle: \"{escaped}\"\ndate: {today}\ndraft: true\ntags: []\n---\n\n");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_err(parent))?;
    }
    fs::write(&path, body).map_err(io_err(&path))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("meteorite-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn new_page_rejects_traversal() {
        let root = tmp("scaffold");
        fs::create_dir_all(root.join("content")).unwrap();
        for bad in ["../evil.md", "/etc/passwd.md", "a/../../b.md", "x.txt"] {
            assert!(new_page(&root, bad, None).is_err(), "{bad}");
        }
        let p = new_page(&root, "blog/my-post", Some("Say \"hi\"")).unwrap();
        assert!(p.ends_with("content/blog/my-post.md"));
        let text = fs::read_to_string(p).unwrap();
        assert!(text.contains("title: \"Say \\\"hi\\\"\""));
        let _ = fs::remove_dir_all(root);
    }
}
