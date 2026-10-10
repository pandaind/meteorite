---
title: Using themes
weight: 1
---
A theme is a directory under `themes/`. Meteorite bundles two, available offline:

| Theme | Look |
|---|---|
| `default` | Clean, readable, light and dark. |
| `terminal` | Monospace, dark, minimal. |

```sh
meteorite theme list
meteorite theme add terminal
```

Then switch in `config.yml`:

```yaml
theme: terminal
```

The same content renders under either theme.

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
