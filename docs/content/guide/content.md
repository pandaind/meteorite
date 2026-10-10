---
title: Writing content
weight: 2
description: Front matter, sections, bundles, summaries and drafts.
---
## Files and URLs

A file's path under `content/` becomes its URL:

| File | URL |
|---|---|
| `content/_index.md` | `/` |
| `content/about.md` | `/about/` |
| `content/blog/_index.md` | `/blog/` (a section listing) |
| `content/blog/hello.md` | `/blog/hello/` |
| `content/blog/hello/index.md` | `/blog/hello/` (a bundle) |

URLs always end in a slash and are written as `dir/index.html`, so they work on any static host.

## Front matter

YAML between `---` lines at the top of a file:

```yaml
---
title: My post
date: 2026-10-10
draft: false
tags: [rust, web]
slug: custom-url-segment
description: Shown in listings, feeds and <meta>.
weight: 10
template: page.html
---
```

| Key | Meaning |
|---|---|
| `title` | Page title. Defaults to a humanised file name. |
| `date` | `YYYY-MM-DD` (RFC 3339 and `YYYY-MM-DD HH:MM:SS` also accepted). Only the calendar date is used. |
| `expiry_date` | The page is hidden from this date on. |
| `draft` | Excluded unless built with `--drafts`. |
| `slug` | Replaces the last URL segment. May not contain `/`, `\`, `:` or `..`. |
| `template` | Use a specific template instead of the default. |
| `weight` | Sort key when a section uses `sort_by: weight`; also a tiebreaker. |
| `tags` and any other list | Taxonomy terms, if the taxonomy is enabled in `config.yml`. |
| anything else | Available to templates as `page.meta.<key>`. |

Section pages (`_index.md`) also accept `sort_by` (`date` newest first, `weight`, or `title`)
and `paginate_by`.

## Bundles

Put a page and its assets together:

```text
content/blog/trip/index.md
content/blog/trip/map.png
```

`map.png` is published next to the page, so `![map](map.png)` just works.

## Summaries, reading time and the table of contents

- Everything before `<!-- more -->` is the page's `summary`; without it, the first paragraph.
- `page.word_count` and `page.reading_time` (minutes, 200 wpm) are computed for you.
- `page.toc` lists headings with `level`, `id` and `title`. Every heading also gets an anchor link.

## Dates and visibility

- Pages dated in the future are skipped unless you build with `--future` (or `build.future: true`).
- Pages past their `expiry_date` are always skipped.
- "Today" is the current date in UTC.

## Code highlighting

Fenced blocks with a known language are highlighted at build time with inline styles, so no
JavaScript is needed. Choose the colour scheme with `markdown.highlight_theme`
(for example `InspiredGitHub`, `base16-ocean.dark`, `Solarized (dark)`), or turn it off with
`markdown.highlight: false`.

## Shortcodes

Shortcodes are reusable snippets rendered from `templates/shortcodes/<name>.html`.

```text
{{< youtube id="dQw4w9WgXcQ" />}}

{{< note kind="warn" >}}
Paired shortcodes pass their Markdown body to the template as `body`.
{{< /note >}}
```

Arguments are `key="string"`, `key=42`, `key=1.5` or `key=true` and become template variables.
Shortcodes are never expanded inside fenced code blocks or inline code, so you can document
their syntax. An unknown name or an unclosed pair is a build error that names the file.
