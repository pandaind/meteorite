---
title: Meteorite
description: A static site generator in Rust. Markdown content, YAML config, drop-in themes, no database.
---
# Meteorite

Write Markdown, configure with YAML, pick a theme, get a static site. Meteorite is a single
binary with no database and no runtime: your site is a folder of files you can keep in git.

```sh
cargo install meteorite
meteorite new site mysite
cd mysite
meteorite serve        # http://127.0.0.1:1111 with live reload
```

## Why

- **Plain files.** Content is `.md`, configuration is `.yml`. Nothing else to run or back up.
- **Plug-and-play themes.** A theme is a folder. Change one line (`theme: terminal`) and the same
  content renders with a different look.
- **Fast.** About 10,000 pages in roughly one second on a modern multi-core machine, with
  incremental rebuilds while you edit.
- **Batteries included.** Sections, pagination, taxonomies, table of contents, syntax
  highlighting, shortcodes, feeds, sitemap, search index, and a link checker.

Start with the [guide](/guide/getting-started/).
