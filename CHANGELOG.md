# Changelog

All notable changes are recorded here. The project follows [Semantic Versioning](https://semver.org);
the template context (`meteorite_api` in `theme.yml`) is versioned separately and only changes
in a major release.

## [Unreleased]

- Fixed: `meteorite serve` wrote its preview into `public/`, so running a production `meteorite build`
  in the same folder replaced the preview's pages with ones that point at the live site and the
  preview came up unstyled. The preview now has its own folder in the system temp directory
- devlog theme 0.1.3: no profile picture is drawn when `avatar` is not set (it used to show a large
  initial); 0.1.2: breadcrumbs show the page's file name instead of a slug of its title, and the
  repository icon stays aligned when a card title wraps

## [0.5.0]

- New bundled theme `orbit` for personal sites: profile home page, projects, blog, micro-blog
  notes, resume (from `data/resume.yml`, print-friendly), a links-in-bio page and social icons.
  Light and dark modes, three skins, search, structured data. `meteorite new site --theme orbit`
  creates a starter with sample content for every part
- New bundled theme `devlog` for software engineers: a repository-style profile with a sidebar,
  pinned project cards, an activity graph, a commit-log blog, notes as file windows and a
  changelog resume. It reads the same content as `orbit`, so switching is one line
- Themes: a section whose `_index.md` sets `full_content: true` puts each page's rendered
  `content` in `section.pages`, `get_pages()` and term pages (for micro-blog streams)

## [0.4.0]

- Chat: new `chat.welcome` option sets the first line shown in the chat window. Without it the
  widget uses the persona's `welcomeMessage` from the PopTalk backend, or a generic greeting
  instead of the previous portfolio wording
- Chat: the bundled widget is PopTalk 1.3.0. Visitors are asked for a name and a valid email
  (or phone) before their first message, and a PopTalk backend older than 1.2 does not enforce
  this; a PopTalk 1.2 or later backend rejects widgets older than 1.2, so upgrade both together

## [0.3.1]

- Fixed: sites built with the bundled themes had no favicon (no icon link, `/favicon.ico` 404).
  Both themes (0.5.1) now ship a `favicon.svg`; override it with a `static/favicon.svg` or the
  `favicon` theme parameter, or set it to `""` to omit the link. Run `meteorite theme update`
  to get it in an existing site

## [0.3.0]

- Optional PopTalk chat assistant (`chat:` in `config.yml`, off by default):
  - `chat_widget()` renders a launcher; the widget loads only when clicked, is self-hosted with
    a subresource integrity hash, and follows the site's light/dark mode and skin accent
  - `chat.knowledge.dir` exports every published page as Markdown with front matter into a
    poptalk-rag knowledge folder (shortcodes and home-page blocks become text; drafts excluded;
    only Meteorite's own files are touched)
  - `meteorite chat check` verifies the backend and CORS; `meteorite chat sync` triggers a
    re-index with a key read from the environment
  - credential-looking keys under `chat:` are refused
- `chat.knowledge.publish: true` also writes the knowledge base as one versioned, deterministic
  `knowledge.json` in the site output, so poptalk-rag can pull it from the live site
  (`KNOWLEDGE_SITE_SOURCES`) instead of needing files copied to its server
- `build --no-export`; the dev theme picker moved to the bottom-left of the page

### Upgrading from 0.2.x

Chat is opt-in and off by default, so nothing changes unless you add a `chat:` block. The bundled
themes gained the `chat_widget()` hook (themes 0.5.0), so run `meteorite theme update` once to
get it; `build` and `serve` warn when the active theme is out of date, and when chat is enabled
but the theme never calls `chat_widget()`.

## [0.2.0]

**Added**

- Search: both bundled themes add a search dialog (press `/` or Ctrl+K, or use the header
  button) when `search.enabled: true`. Ranked results with highlighted matches; the index is
  fetched on first use. `new site` enables it
- Theme switching: `--theme <name>` on `build`, `serve` and `check` overrides the theme in
  `config.yml`. While serving, a dev-only picker re-renders the same URL under any installed
  theme and rebuilds every theme you have looked at on change
- Skins: a theme can declare named palettes in `theme.yml`; visitors choose one from a header
  menu and the choice is remembered. `default` ships `meteor`, `paper` and `aurora`; `terminal`
  ships `green`, `amber` and `mono`. New params `skin` and `skin_switcher`; templates get a
  `theme` object (`name`, `version`, `skins`)
- `meteorite theme update [name]` refreshes installed bundled themes to the version in the
  running release, keeping the old copy in `themes/.backup/`; `theme list` marks outdated themes

**Improved**

- Configuration typos are reported: an unknown key in `config.yml` produces a warning with a
  "did you mean" suggestion instead of being silently ignored
- Template errors point at the problem: syntax errors show the source line with a marker, and
  render errors (for example an undefined variable) say which template line is likely at fault
- `build` and `serve` warn when the active bundled theme is older than the one in the release
  (an existing site keeps its own copy of a theme), and when `params.skin` names no skin

### Upgrading from 0.1.x

A site keeps its own copy of each theme, so existing sites do not pick up the new theme
features by themselves. Run `meteorite theme update` once; your old copy is kept in
`themes/.backup/`. `build` and `serve` warn when the active theme is out of date.

## [0.1.1]

- Package description no longer compares the project to other generators
- Docs: security page wording
- Release workflow builds Intel macOS binaries on the Apple Silicon runner (the old runner
  image was retired) and can be re-run for an existing tag

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
- Redesigned `default` theme (0.3.0) for marketing and docs: sticky header with optional
  GitHub link and call-to-action button, a front-matter-driven home page (hero with copyable
  install command, stats, feature grid, showcase, quote, latest posts, CTA), card-grid listings,
  article layout with a sticky table of contents, code copy buttons, and Open Graph tags.
  New theme parameters: `github_url`, `header_cta_text`, `header_cta_url`, `logo`, `og_image`
- `terminal` theme (0.3.0) given the same treatment: front-matter home page, cards, article
  layout with table of contents, copy buttons, Open Graph tags, and a terminal-window install
  command with a blinking cursor. It reads the same front matter and parameters as `default`
  (breaking for `terminal` users: `accent` is now the light-mode colour and `accent_dark` the
  dark-mode one, matching `default`)
- Hero background: 16 falling meteors with glowing heads and a twinkling star field, in both
  themes; disabled for `prefers-reduced-motion`
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
