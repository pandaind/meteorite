---
title: Getting started
weight: 1
description: Create a site, add a page, and see it live.
---
## Install

```sh
cargo install meteorite
# optional: image resizing
cargo install meteorite --features images
```

## Create a site

```sh
meteorite new site mysite
cd mysite
meteorite serve
```

Open <http://127.0.0.1:1111>. Edit any file under `content/` and the browser reloads.
`serve` keeps running after a mistake: errors are printed in the terminal and the last good
build stays up.

## What you get

```text
mysite/
├── config.yml        site configuration
├── content/          your Markdown
├── data/             optional YAML data files
├── static/           copied as-is
├── templates/        optional overrides for theme templates
└── themes/default/   the active theme
```

## Add a page

```sh
meteorite new page blog/my-first-post --title "My first post"
```

This creates `content/blog/my-first-post.md` as a draft. Set `draft: false` (or run
`meteorite serve --drafts` while writing) to publish it.

## Build for production

```sh
meteorite build            # writes public/
meteorite check            # verifies internal links
```

Set `base_url` in `config.yml` to your real address first: feeds and the sitemap need an
absolute URL. See [Deployment](/guide/deployment/).
