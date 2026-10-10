---
title: The orbit theme
weight: 3
description: A personal site with a profile, projects, blog, micro-blog, resume and links page.
---
`orbit` is for a person rather than a product: one site that holds your profile, the things you
have built, longer writing, short notes, your resume and a "link in bio" page.

```sh
meteorite new site me --theme orbit
cd me && meteorite serve
```

The starter has sample content for every part, so you only replace the words.

## What is in it

| Part | Where | Notes |
|---|---|---|
| Profile home page | `content/_index.md` | Name, role, headline, availability badge, buttons, socials, stats |
| Projects | `content/projects/` | `layout: projects` in `_index.md`; cards with cover, year and stack |
| Blog | `content/blog/` | Dates, tags, reading time, pagination, feed |
| Micro-blog | `content/notes/` | `layout: notes` and `full_content: true`; short untitled posts shown in full |
| Resume | `content/resume.md` + `data/resume.yml` | Timeline, skills, print-friendly, optional PDF download |
| Links page | `content/links.md` + `data/socials.yml` | Big buttons for a link in your bio |
| Socials | `data/socials.yml` | Icons on the home page, footer and links page |

It also gives you light and dark modes, search, a feed, social sharing tags, structured data
(`Person`) for search engines and the optional [chat assistant](/guide/chat/).

## The home page

Everything comes from front matter in `content/_index.md`. Every block is optional.

```yaml
profile:
  greeting: Hi, I'm
  name: Ada Lovelace            # defaults to the site title
  role: Software engineer
  headline: I build calm, fast things for the web.
  status: { text: Open to new projects, state: open, url: /links/ }   # state: open | busy
  actions:
    - { name: See my work, url: /projects/, style: primary }
    - { name: Resume, url: /resume/, style: secondary }
stats:
  - { value: 8 years, label: building for the web }
sections: [projects, posts, notes, skills, experience, contact]   # order; omit one to hide it
projects: { title: Selected work, limit: 3 }
posts: { title: Writing, limit: 3 }
notes: { title: Notes, limit: 3 }
skills: { title: Skills, items: [Rust, TypeScript] }   # or omit items to use data/resume.yml
contact: { title: Let's work together, text: Tell me what you are building. }
```

The Markdown below the front matter becomes the short bio. Set `hide_socials: true` under
`profile` to leave out the social icons.

## Section layouts

A section picks its look with `layout:` in its `_index.md`:

| `layout` | Looks like |
|---|---|
| `list` (default) | Blog posts: title, summary, date, reading time, tags |
| `projects` | A grid of cards |
| `notes` | A stream of short posts, shown in full |
| `archive` | Dates and titles only |

Pages inside a `projects` section get a project page (cover, role, year, stack, links) and
pages inside `notes` get a single-note page. A page can also choose its own: `layout: resume`,
`layout: links`, `layout: article`.

### Projects

```yaml
---
title: Faster search
description: One sentence for the card.
year: 2025
role: Design and engineering
stack: [Rust, SQLite]
cover: projects/search.jpg     # image under static/
repo: https://github.com/you/search
demo: https://example.com
weight: 1
---
```

### Notes

A note is just a Markdown file with a date. It shows no heading unless you set
`show_title: true`. Add `link: https://...` to share a link.

```yaml
---
date: 2026-03-04
tags: [rust]
---
Learned that `cargo install --locked` avoids surprise upgrades.
```

## Data files

`data/socials.yml` is a list; `icon` is one of `github`, `linkedin`, `x`, `mastodon`,
`bluesky`, `youtube`, `instagram`, `mail`, `rss`, `globe` (anything else shows a link icon).

```yaml
- { name: GitHub, url: https://github.com/you, icon: github, handle: "@you" }
```

`data/resume.yml` holds `basics`, `work`, `education`, `skills`, `projects`, `languages` and
`certifications`; see the starter for the shape. Add `resume_pdf: resume.pdf` under `params` (the
file goes in `static/`) for a Download button. The page also prints cleanly from the browser.

## Parameters

| Parameter | Default | Meaning |
|---|---|---|
| `skin` | `ink` | `ink`, `sunset` or `mint` |
| `skin_switcher` | `false` | Let visitors pick a skin |
| `color_scheme` | `auto` | `auto`, `light` or `dark` |
| `theme_toggle` | `true` | Light/dark button in the header |
| `tagline` | none | One line used on the links page and in sharing cards |
| `avatar` | none | Your photo, under `static/` |
| `location`, `email` | none | Shown on the home page and resume |
| `resume_pdf` | none | PDF under `static/` for the Download button |
| `blog_section`, `notes_section`, `projects_section` | `blog`, `notes`, `projects` | Where the home page looks |
| `reading_time` | `true` | Show "n min read" |
| `accent`, `accent_dark` | skin | Override the accent colour |
| `og_image`, `favicon`, `footer_text` | none / `favicon.svg` / none | Sharing image, icon, footer text |
