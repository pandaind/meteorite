# Changelog

All notable changes are recorded here. The project follows [Semantic Versioning](https://semver.org);
the template context (`meteorite_api` in `theme.yml`) is versioned separately and only changes
in a major release.

## [Unreleased]

## [0.1.0]

First release.

- `new site`, `new page`, `build`, `serve`, `check`, `clean`, `theme list|add|remove`
- Markdown + YAML front matter; sections, bundles, drafts, future and expiry dates
- Themes with `theme.yml` manifest, parameters and API versioning; `default` and `terminal` bundled
- Site-level template and static overrides
- Pagination, taxonomies, menus, permalinks, data files
- Heading anchors, table of contents, syntax highlighting, summaries, reading time
- Shortcodes that are never expanded inside code
- Atom/RSS feeds, sitemap, robots.txt, search index, HTML/CSS minification
- Optional image resizing (`--features images`)
- Dev server with file watching, incremental rebuilds and WebSocket live reload
- Diagnostics with source snippets for YAML errors
