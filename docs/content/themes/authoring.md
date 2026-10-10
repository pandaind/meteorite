---
title: Writing a theme
weight: 2
---
## Layout

```text
themes/mytheme/
├── theme.yml
├── templates/
│   ├── base.html
│   ├── index.html        home page
│   ├── page.html         a content page
│   ├── section.html      a section listing
│   ├── taxonomy.html     all terms of a taxonomy      (only if you use taxonomies)
│   ├── term.html         pages for one term           (only if you use taxonomies)
│   ├── 404.html          optional
│   └── shortcodes/       one file per shortcode
└── static/               copied to the site root
```

Templates use [Tera](https://keats.github.io/tera/docs/) (Jinja-like). Output is HTML-escaped
by default; use `| safe` for `page.content`, `page.summary` and shortcode `body`.

## theme.yml

```yaml
name: mytheme            # letters, digits, - and _
version: 1.0.0
description: What it looks like
author: You
meteorite_api: 1         # template context version, see below
params:
  accent:
    default: "#5b4de0"
    description: Link colour
  tagline:
    required: true       # the build fails with a clear message if unset
```

`meteorite_api` is the version of the template context and file conventions the theme was
written for. Meteorite refuses to load a theme that targets a different version, so an
incompatible theme fails up front instead of rendering wrongly.

## Template context (API v1)

Available in every template:

| Name | Type | Notes |
|---|---|---|
| `config` | object | The merged site config. `config.params` includes theme defaults. |
| `data` | object | Contents of `data/*.yml`, keyed by file name. |
| `meteorite.version` | string | |
| `page` | object | The current page (always set; synthetic for listings). |

Functions:

| Function | Returns |
|---|---|
| `url(path="/blog/")` | The path joined to `base_url`. Always use this for links so sub-path hosting works. |
| `get_pages(section="", limit=N, recursive=true)` | Pages (no body), newest first. |
| `get_section(path="blog")` | A section object including its `pages`. |
| `resize_image(path="a.jpg", width=400)` | URL of a resized copy (needs `--features images`). |

### `page`

| Field | Notes |
|---|---|
| `meta` | Front matter: `title`, `description`, `tags`, `weight`, plus any custom keys. |
| `url`, `permalink` | Root-relative URL and absolute URL. |
| `content` | Rendered HTML body. |
| `summary` | HTML before `<!-- more -->`, or the first paragraph. |
| `date` | `YYYY-MM-DD` or null. |
| `word_count`, `reading_time` | Integers. |
| `toc` | List of `{level, id, title}`. |
| `section` | Path of the containing section. |
| `prev`, `next` | `{title, url}` or null, following the section's order. |

### Per template

| Template | Extra variables |
|---|---|
| `page.html` | `section` = `{path, url, title}` of the parent section |
| `index.html`, `section.html` | `section` (below), `paginator`, `page` = the `_index.md` page |
| `taxonomy.html` | `taxonomy` = `{name, url}`, `terms` = `[{name, slug, url, count}]` |
| `term.html` | `taxonomy`, `term` = `{name, slug, url, count}`, `pages` |
| `404.html` | nothing extra |

`section`: `path`, `url`, `permalink`, `title`, `description`, `meta`, `content`, `toc`,
`subsections` (`{path, url, title, description}`), `pages` (this page of the listing, bodies
omitted) and `total_pages`.

`paginator`: `number`, `total_pages`, `per_page`, `total_items`, `current_url`, `first_url`,
`last_url`, `prev_url` and `next_url` (null at the ends). It is always present; a section
without pagination has `total_pages == 1`.

### Shortcodes

`templates/shortcodes/note.html` is invoked as `{{< note kind="warn" >}}...{{< /note >}}`.
Arguments arrive as top-level variables and the Markdown body, already rendered, as `body`.

## Tips

- Link with `url(path=...)`, never a hard-coded `/`.
- For the home page's latest posts use `get_pages(limit=5)`; don't loop over everything.
- Keep a `static/` stylesheet name unique to your theme to avoid clashing with a site's files.
- Test with `meteorite check`: it catches links your templates build wrongly.
