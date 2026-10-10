---
title: Using themes
weight: 1
---
A theme is a directory under `themes/`. Meteorite bundles two, available offline:

| Theme | Look |
|---|---|
| `default` | Clean and readable. Light and dark modes. |
| `terminal` | Monospace terminal look: prompt and `[nav]` links, a terminal window for the install command, blinking cursor. Dark by default, with a light mode. |

```sh
meteorite theme list
meteorite theme add terminal
```

Then switch in `config.yml`:

```yaml
theme: terminal
```

The same content renders under either theme.

## Light and dark mode

Both bundled themes have a toggle button in the header. The visitor's choice is remembered in
their browser, and applied before the page paints so there is no flash. Code blocks switch
palette with the page.

Choose the starting mode, or hide the button, with theme parameters:

```yaml
params:
  color_scheme: auto    # auto = follow the OS | light | dark
  theme_toggle: true    # false hides the button
```

Both themes start in `dark`. A visitor's saved choice always wins.

## Installing third-party themes

```sh
meteorite theme add ./my-theme
meteorite theme add https://github.com/someone/meteorite-theme-foo.git --name foo
```

Meteorite validates `theme.yml` before installing anything and leaves no partial install behind.
Symlinks and `.git` are stripped. Themes are templates and static files only; they cannot run
code. Raw HTML and `<script>` tags in a theme's templates do end up in your site, so read a
theme before you trust it with a production site.

## Site icon

Both bundled themes ship a `favicon.svg`. To use your own, put a file of the same name in your
site's `static/` directory (it overrides the theme's), or point the `favicon` parameter at another
file such as `icon.png`. Set `favicon: ""` for no icon link at all.

## Upgrading bundled themes

A site keeps its own copy of a theme in `themes/<name>/`, made when you installed it. Upgrading
Meteorite therefore does **not** change an existing site's theme, and a theme copied from an older
release can lack newer features (skins, search, the style menu) while a theme you add today has
them.

Meteorite tells you: `theme list` marks outdated bundled themes, and `build` and `serve` print a
warning for the active one. To refresh:

```sh
meteorite theme list                 # shows: [update available: 0.4.0]
meteorite theme update               # every outdated bundled theme
meteorite theme update default       # or just one
```

The previous copy is moved to `themes/.backup/<name>-<version>-<timestamp>/`, never deleted, so
local edits can be recovered. To keep customisations across updates, put them in your site's
own `templates/` and `static/` directories, which always override the theme and are never touched.
Only bundled themes can be updated this way; reinstall a third-party theme with `theme remove`
and `theme add`.

## Previewing other themes

You don't have to edit `config.yml` to try a theme. Use `--theme` on `build`, `serve` or `check`:

```sh
meteorite serve --theme terminal
meteorite build --theme terminal --output /tmp/preview
```

While `meteorite serve` is running with two or more themes installed, a small picker appears
in the bottom-right corner of every page. Choose another theme and the same URL is re-rendered
with it, instantly; edits rebuild every theme you've looked at. The picker exists only in the
dev server and is never part of a build. Your `config.yml` is not touched.

## Skins: change the look without changing the theme

A theme can ship several **skins**: named palettes (and sometimes a heading typeface) that change
the colours but not the markup. Both bundled themes do:

| Theme | Skins |
|---|---|
| `default` | `meteor` (default), `paper` (warm, serif headings), `aurora` (teal and violet) |
| `terminal` | `green` (default), `amber`, `mono` |

Visitors get a palette button in the header; their choice is remembered like the light/dark
mode, and every skin works in both modes. Set the starting skin, or hide the menu:

```yaml
params:
  skin: paper
  skin_switcher: false
```

An unknown skin name produces a warning that lists the available ones.

## Customising without forking

Anything in your site's own `templates/` or `static/` directory wins over the theme's file of
the same name. To change just the footer of `base.html`, copy that one file into
`templates/` and edit it.

## The home page

Both bundled themes build the home page from front matter in `content/_index.md`. Every block is
optional: `hero`, `stats`, `features`, `showcase`, `quote`, `latest` and `cta`. The page body
(Markdown) is shown between the hero and the features.

```yaml
---
title: Welcome
hero:
  eyebrow: Built with Meteorite
  title: Your site, written in plain Markdown
  subtitle: "Short supporting sentence."
  command: cargo install meteorite     # shows a copy-able command
  actions:
    - { name: Get started, url: /guide/, style: primary, icon: rocket }
    - { name: GitHub, url: "https://github.com/you/repo", style: secondary }
stats:
  - { value: "~1 s", label: "to build 10,000 pages" }
features:
  title: Why this
  items:
    - { icon: bolt, title: Fast, text: "One sentence, quoted if it has a comma." }
cta:
  title: Ready?
  actions:
    - { name: Start, url: /guide/ }
---
```

Quote any value that contains a comma or a colon. Inside `{ ... }` an unquoted comma ends the
value, so `text: fast, simple` would silently become `text: fast`.

Available icons: `bolt`, `folder`, `palette`, `search`, `code`, `shield`, `rocket`, `globe`,
`book`, `layers`, `check`, `arrow`, `download`, `play`, `copy`, `heart`, `star`.

Header options live under `params:` in `config.yml`: `github_url`, `header_cta_text`,
`header_cta_url`, `logo`, `og_image`, `favicon`, `footer_text`, plus the colour options above. The two
themes share this front-matter schema and these parameters (`accent` is the light-mode colour and
`accent_dark` the dark-mode one in both), so switching `theme:` keeps your home page intact.
`terminal` adds `prompt` (the character before the title).

The hero has a falling-meteor background. It is decorative, hidden from assistive technology, and
turned off for visitors who prefer reduced motion.
