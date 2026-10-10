---
title: The devlog theme
weight: 4
description: A software engineer's site in the style of a code repository.
---
`devlog` is for software engineers. It dresses the same content as [`orbit`](/themes/orbit/)
like a code repository: your profile is the sidebar, and the rest reads like a project you
are proud of.

```sh
meteorite new site me --theme devlog
cd me && meteorite serve
```

## How it looks

| Content | Shown as |
|---|---|
| Profile | A repo-owner sidebar: avatar, name, `handle`, role, status, location, socials, stack |
| Home bio | `README.md` panel |
| Projects | Repository cards with language dots (from `stack`), year and links |
| Activity | A contribution graph built from the dates of your pages |
| Blog | A commit-style log: date, title, tags, reading time |
| Notes | Small `2026-03-04.md` file windows |
| Resume | A changelog-style timeline from `data/resume.yml`, print-friendly |
| Links | The same links-in-bio page as `orbit` |
| Footer | An editor status bar with the branch, feed and Meteorite version |

Navigation is a row of editor tabs, and every inner page has a `~/section/page` path above it.

## Content and parameters

Front matter, section layouts (`projects`, `notes`, `archive`, `list`), `data/socials.yml` and
`data/resume.yml` are exactly as described for [`orbit`](/themes/orbit/). Extra to `devlog`:

| Parameter | Default | Meaning |
|---|---|---|
| `handle` | none | Username shown under your name |
| `show_activity` | `true` | The activity graph on the home page |
| `skin` | `slate` | `slate`, `dracula` or `gruvbox` |

The `activity` block in `sections:` places the graph; the default order is
`[projects, activity, posts, notes, contact]`. The graph ends at the date of your newest page, not
at today, so the same content always draws the same picture. Skills listed under `skills.items` in
the home page's front matter appear in the sidebar with language dots.
