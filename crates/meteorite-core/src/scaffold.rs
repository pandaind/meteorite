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

search:
  enabled: true            # adds a search box (press / or Ctrl+K) to the theme

markdown:
  highlight: true
  highlight_theme: InspiredGitHub          # code colours in light mode
  highlight_theme_dark: base16-ocean.dark  # code colours in dark mode

build:
  drafts: false
  minify: false

# Optional: an AI or human chat assistant on every page, using PopTalk
# (https://github.com/pandaind/poptalk). Uncomment and point it at your backend.
# chat:
#   provider: poptalk
#   api_url: https://chat.example.com
#   persona: my-site            # a persona id on a multi-persona backend (optional)
#   knowledge:                  # export this site as poptalk-rag's knowledge base
#     dir: ../poptalk-rag/knowledge/my-site
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

/// The starter for the personal-site themes (`orbit`, `devlog`): a profile, projects, a blog,
/// micro-blog notes, a resume and a links page. Both themes read the same content.
fn personal_files(theme: &str, today: &str) -> Vec<(&'static str, String)> {
    // A few dates spread over the past months, so the activity graph has something to draw.
    let ago = |days: i64| -> String {
        (chrono::Utc::now() - chrono::Duration::days(days))
            .format("%Y-%m-%d")
            .to_string()
    };
    let config = "\
# Site configuration. Docs: https://github.com/pandaind/meteorite
title: Your Name
# Set this to your production URL. `meteorite serve` overrides it locally.
base_url: https://example.com/
language: en
theme: orbit

params:
  tagline: Software engineer, writer and maker
  location: Your City
  email: you@example.com
  # avatar: avatar.jpg            # put the file in static/
  # resume_pdf: resume.pdf        # a PDF in static/ adds a Download button to /resume/
  # color_scheme: auto            # auto | light | dark
  # skin: ...                     # see the theme's skins in themes/<name>/theme.yml
  # skin_switcher: true           # let visitors pick a skin

menus:
  main:
    - { name: Work, url: /projects/, weight: 1 }
    - { name: Writing, url: /blog/, weight: 2 }
    - { name: Notes, url: /notes/, weight: 3 }
    - { name: Resume, url: /resume/, weight: 4 }

taxonomies: [tags]
paginate_by: 10

feed:
  enabled: true

search:
  enabled: true

markdown:
  highlight: true
  highlight_theme: InspiredGitHub
  highlight_theme_dark: base16-ocean.dark

build:
  drafts: false
  minify: false

# Optional: an AI or human chat assistant on every page, using PopTalk
# (https://github.com/pandaind/poptalk). Uncomment and point it at your backend.
# chat:
#   provider: poptalk
#   api_url: https://chat.example.com
#   welcome: Ask me about my work or writing
"
    .replace("theme: orbit", &format!("theme: {theme}"));

    let home = "\
---
title: Home
description: Software engineer, writer and maker.
profile:
  greeting: Hi, I'm
  name: Your Name
  role: Software engineer
  headline: I build calm, fast things for the web and write about how they work.
  status: { text: Open to new projects, state: open, url: /links/ }
  actions:
    - { name: See my work, url: /projects/, style: primary }
    - { name: Resume, url: /resume/, style: secondary }
stats:
  - { value: 8 years, label: building for the web }
  - { value: 40+, label: projects shipped }
  - { value: 120, label: posts and notes }
sections: [projects, activity, posts, notes, skills, contact]
projects: { title: Selected work, limit: 4 }
posts: { title: Writing, limit: 3 }
notes: { title: Notes, limit: 3 }
skills:
  title: What I work with
  items: [Rust, TypeScript, Python, PostgreSQL, Accessibility, Performance, Technical writing, Design systems]
contact:
  title: Let's work together
  text: I take on a few projects each year. Tell me what you are building.
---
A short paragraph about you goes here. Say what you do, who you do it for and what you care
about. Edit it in `content/_index.md`.
"
    .to_string();

    let projects_index = "\
---
title: Work
description: Things I have designed and built.
layout: projects
sort_by: weight
---
"
    .to_string();

    let project = |title: &str, weight: u32, year: u32, desc: &str, stack: &str, date: &str| {
        format!(
            "\
---
title: {title}
date: {date}
description: {desc}
weight: {weight}
year: {year}
role: Design and engineering
stack: [{stack}]
# cover: projects/{slug}.jpg     # an image in static/
# repo: https://github.com/you/{slug}
# demo: https://example.com
---
## The problem

Describe what you set out to solve and for whom.

## What I built

Explain the approach, the decisions that mattered and what you would do differently.

## Result

Numbers, quotes or links that show it worked.
",
            slug = title.to_lowercase().replace(' ', "-")
        )
    };

    let blog_index = "\
---
title: Writing
description: Longer posts on building software.
sort_by: date
paginate_by: 8
---
"
    .to_string();

    let hello = format!(
        "\
---
title: Hello, world
date: {today}
tags: [intro]
description: Why this site exists and what to expect here.
---
Welcome. This is the first post on my new site.

<!-- more -->

## What you will find here

Long-form writing lives in `content/blog`. Short thoughts go in `content/notes`, and the things
I have built are under `content/projects`.

```rust
fn main() {{
    println!(\"hello from my site\");
}}
```
"
    );

    let notes_index = "\
---
title: Notes
description: Short thoughts, links and things I am learning.
layout: notes
full_content: true
sort_by: date
paginate_by: 10
---
A micro-blog. No titles needed, just write.
"
    .to_string();

    let note = |text: &str, date: &str, tags: &str| {
        format!("---\ndate: {date}\ntags: [{tags}]\n---\n{text}\n")
    };

    let resume = "\
---
title: Resume
description: Experience, education and skills.
layout: resume
---
"
    .to_string();

    let links = "\
---
title: Your Name
description: Find me elsewhere.
layout: links
links:
  - { title: Read the blog, url: /blog/, description: Longer posts, icon: book }
  - { title: See my work, url: /projects/, description: Selected projects, icon: code }
---
"
    .to_string();

    let socials = "\
# Shown on the home page, the footer and the links page. `icon` is one of: github, linkedin, x,
# mastodon, bluesky, youtube, instagram, mail, rss, globe.
- { name: GitHub, url: https://github.com/you, icon: github, handle: \"@you\" }
- { name: LinkedIn, url: https://www.linkedin.com/in/you, icon: linkedin, handle: you }
- { name: Mastodon, url: https://mastodon.social/@you, icon: mastodon, handle: \"@you@mastodon.social\" }
- { name: Email, url: \"mailto:you@example.com\", icon: mail, handle: you@example.com }
"
    .to_string();

    let resume_data = "\
basics:
  name: Your Name
  label: Software engineer
  email: you@example.com
  location: Your City
  url: https://example.com
  summary: Engineer with a focus on fast, accessible web products and clear writing.
work:
  - company: Company Name
    role: Senior Engineer
    start: 2022
    location: Remote
    summary: What the team does and your part in it.
    highlights:
      - Cut page load time by 40% by rebuilding the rendering pipeline
      - Led a team of four through a platform migration
  - company: Earlier Company
    role: Engineer
    start: 2018
    end: 2022
    highlights:
      - Shipped the first version of the customer dashboard
education:
  - school: University Name
    degree: BSc Computer Science
    start: 2014
    end: 2018
skills:
  - { group: Languages, items: [Rust, TypeScript, Python, SQL] }
  - { group: Practices, items: [Accessibility, Performance, Technical writing] }
languages:
  - { name: English, level: Native }
"
    .to_string();

    vec![
        ("config.yml", config),
        ("content/_index.md", home),
        ("content/projects/_index.md", projects_index),
        (
            "content/projects/first-project.md",
            project(
                "First project",
                1,
                2025,
                "A one-line summary of what it is and why it matters.",
                "Rust, WebAssembly",
                &ago(21),
            ),
        ),
        (
            "content/projects/second-project.md",
            project(
                "Second project",
                2,
                2024,
                "Another thing you made, in a sentence.",
                "TypeScript, PostgreSQL",
                &ago(95),
            ),
        ),
        ("content/blog/_index.md", blog_index),
        ("content/blog/hello-world.md", hello),
        ("content/notes/_index.md", notes_index),
        (
            "content/notes/first-note.md",
            note(
                "Notes are short. Write one in a minute, with *Markdown*, a [link](https://example.com) or a code `snippet`.",
                &ago(3),
                "meta",
            ),
        ),
        (
            "content/notes/reading.md",
            note(
                "Currently reading something good, and taking notes in public.",
                &ago(12),
                "reading",
            ),
        ),
        (
            "content/notes/til.md",
            note(
                "TIL: `cargo install --locked` avoids surprise dependency upgrades.",
                &ago(40),
                "rust",
            ),
        ),
        ("content/resume.md", resume),
        ("content/links.md", links),
        ("data/socials.yml", socials),
        ("data/resume.yml", resume_data),
        (".gitignore", "public/\n".to_string()),
    ]
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
    let files: Vec<(&str, String)> = if theme == "orbit" || theme == "devlog" {
        personal_files(theme, &today)
    } else {
        vec![
            ("config.yml", config_yml(theme)),
            ("content/_index.md", HOME.to_string()),
            ("content/about.md", ABOUT.to_string()),
            ("content/blog/_index.md", BLOG_INDEX.to_string()),
            ("content/blog/hello-world.md", hello_post(&today)),
            (".gitignore", "public/\n".to_string()),
        ]
    };
    for (rel, body) in files {
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
