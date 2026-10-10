# Changelog

All notable changes are recorded here. The project follows [Semantic Versioning](https://semver.org);
the template context (`meteorite_api` in `theme.yml`) is versioned separately and only changes
in a major release.

## [Unreleased]

- Redesigned `default` theme (0.3.0) for marketing and docs: sticky header with optional
  GitHub link and call-to-action button, a front-matter-driven home page (hero with copyable
  install command, stats, feature grid, showcase, quote, latest posts, CTA), card-grid listings,
  article layout with a sticky table of contents, code copy buttons, and Open Graph tags.
  New theme parameters: `github_url`, `header_cta_text`, `header_cta_url`, `logo`, `og_image`
- Output files are written atomically (temp file + rename), so the dev server never serves a
  half-written file
- `new site` starts from a home page that uses the marketing blocks

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
