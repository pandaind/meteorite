---
title: Using themes
weight: 1
---
A theme is a directory under `themes/`. Meteorite bundles two, available offline:

| Theme | Look |
|---|---|
| `default` | Clean and readable. Light and dark modes. |
| `terminal` | Monospace and minimal. Dark by default, with a light mode. |

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

`default` starts in `auto`; `terminal` starts in `dark`. A visitor's saved choice always wins.

## Installing third-party themes

```sh
meteorite theme add ./my-theme
meteorite theme add https://github.com/someone/meteorite-theme-foo.git --name foo
```

Meteorite validates `theme.yml` before installing anything and leaves no partial install behind.
Symlinks and `.git` are stripped. Themes are templates and static files only; they cannot run
code. Raw HTML and `<script>` tags in a theme's templates do end up in your site, so read a
theme before you trust it with a production site.

## Customising without forking

Anything in your site's own `templates/` or `static/` directory wins over the theme's file of
the same name. To change just the footer of `base.html`, copy that one file into
`templates/` and edit it.

## The home page

The default theme builds the home page from front matter in `content/_index.md`. Every block is
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
`header_cta_url`, `logo`, `og_image`, `footer_text`, plus the colour options above.
