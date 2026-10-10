---
title: Meteorite
description: A static site generator in Rust. Markdown content, YAML config, drop-in themes, no database.
hero:
  announcement: { text: "New: light and dark mode in every theme", url: /themes/using/ }
  title: Static sites from plain Markdown
  subtitle: "Meteorite is a fast static site generator written in Rust. Write content in Markdown, configure with YAML, and swap themes in one line. No database, no runtime, no build chain."
  command: cargo install meteorite
  actions:
    - { name: Get started, url: /guide/getting-started/, style: primary, icon: rocket }
    - { name: Read the docs, url: /guide/, style: secondary, icon: book }
stats:
  - { value: "~1 s", label: "to build 10,000 pages" }
  - { value: "1", label: "binary, nothing else to install" }
  - { value: "0", label: "databases or servers to run" }
  - { value: "2", label: "bundled themes, light and dark" }
features:
  title: Everything a site needs, nothing it doesn't
  subtitle: "A single folder of Markdown and YAML in, a folder of static files out."
  items:
    - { icon: folder, title: "Plain files, in git", text: "Pages are Markdown with YAML front matter. There is no database to back up and nothing to keep running.", url: /guide/content/, link_text: "Writing content" }
    - { icon: palette, title: "Themes that just plug in", text: "A theme is a folder. Change one line and the same content renders with a different look. Your own templates override the theme without forking it.", url: /themes/using/, link_text: "Using themes" }
    - { icon: bolt, title: "Fast, with live reload", text: "Parallel builds, and rebuilds that only touch what changed while you edit. The browser reloads on save.", url: /reference/performance/, link_text: "See the numbers" }
    - { icon: code, title: "Code that reads well", text: "Build-time syntax highlighting with separate light and dark palettes, plus copy buttons. No client-side JavaScript required.", url: /guide/content/, link_text: "Highlighting" }
    - { icon: layers, title: "Sections, tags and pagination", text: "Nested sections, taxonomies, menus, permalinks, data files, a table of contents and shortcodes are built in.", url: /guide/organising/, link_text: "Organising content" }
    - { icon: shield, title: "Safe to publish", text: "A link checker catches broken pages before your readers do, and the build refuses paths that could escape the output directory.", url: /reference/security/, link_text: "Security model" }
showcase:
  eyebrow: Configuration you can read
  title: One file describes your site
  text: "Everything lives in plain YAML next to your content. Pick a theme, set your menus, turn on feeds and search."
  points:
    - "Atom and RSS feeds, a sitemap and robots.txt, generated for you"
    - "A client-side search index you can load from any theme"
    - "Optional HTML and CSS minification for production"
  url: /reference/config/
  link_text: "Configuration reference"
  code_title: config.yml
  code: |
    title: My Site
    base_url: https://example.com/
    theme: default

    menus:
      main:
        - { name: Docs, url: /guide/ }

    taxonomies: [tags]
    paginate_by: 10
    search:
      enabled: true
cta:
  title: Build your first site in a minute
  text: "Install, scaffold, and serve. Edit a file and watch the browser update."
  actions:
    - { name: Read the guide, url: /guide/getting-started/, style: primary }
    - { name: Command line reference, url: /reference/cli/, style: secondary }
---
