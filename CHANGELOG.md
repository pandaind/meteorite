# Changelog

All notable changes are recorded here. The project follows [Semantic Versioning](https://semver.org);
the template context (`meteorite_api` in `theme.yml`) is versioned separately and only changes
in a major release.

## [Unreleased]

- Light and dark mode in both bundled themes: a header toggle, remembered per visitor,
  applied before first paint; `color_scheme` and `theme_toggle` theme parameters
- Code highlighting now emits CSS classes and a generated `highlight.css` with a light and a
  dark palette (`markdown.highlight_style: classes`, the default). Themes must link
  `highlight.css`. `highlight_style: inline` keeps the previous single-palette behaviour
- `markdown.highlight_theme_dark`; unknown highlight theme names are now an error that lists
  the available ones instead of silently falling back

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
