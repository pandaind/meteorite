---
title: Deployment
weight: 4
---
`meteorite build` produces a plain directory (`public/`). Upload it anywhere that serves files.

## Before you ship

1. Set `base_url` to the final address, including any sub-path
   (`https://example.com/` or `https://user.github.io/project/`).
2. Run `meteorite check`. It builds into a scratch directory and fails if an internal link or
   image points at something that doesn't exist.
3. Optionally minify: `meteorite build --minify`.

## GitHub Pages

```yaml
name: Deploy
on: { push: { branches: [main] } }
permissions: { pages: write, id-token: write, contents: read }
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo install meteorite
      - run: meteorite check && meteorite build
      - uses: actions/upload-pages-artifact@v3
        with: { path: public }
  deploy:
    needs: build
    runs-on: ubuntu-latest
    environment: github-pages
    steps:
      - uses: actions/deploy-pages@v4
```

## Other hosts

Netlify, Cloudflare Pages, S3 + CDN and plain nginx all work the same way: publish `public/`.
If the host supports a custom 404 page, point it at `404.html`.

## Generated files

| File | When |
|---|---|
| `atom.xml`, `rss.xml` | `feed.enabled` (default) and at least one dated page |
| `sitemap.xml` | `sitemap: true` (default) |
| `robots.txt` | always, unless you supply `static/robots.txt` |
| `search-index.json` | `search.enabled: true` |

Feeds and the sitemap need an absolute `base_url`; with a relative one they are skipped and
the build prints a warning.

## Search

With `search.enabled: true` the build writes `search-index.json`
(`[{"title","url","description","tags","text"}]`), and both bundled themes add a search box:
press `/` or <kbd>Ctrl</kbd>+<kbd>K</kbd>, or use the magnifier in the header. Results are
ranked (title, then tags, description and body), every word must match, and the index is only
downloaded the first time someone searches.

To build your own interface, load the index from any theme:

```js
const index = await (await fetch("/search-index.json")).json();
const hits = index.filter(p =>
  (p.title + " " + p.text).toLowerCase().includes(query.toLowerCase()));
```
