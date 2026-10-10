---
title: Sections, taxonomies and menus
weight: 3
---
## Sections

Every directory under `content/` that contains pages is a section and gets a listing page at
its URL. Add an `_index.md` to give it a title, intro text, sort order or page size:

```yaml
---
title: Blog
sort_by: date      # date (default, newest first) | weight | title
paginate_by: 10
---
Everything I've written.
```

Pagination produces `/blog/`, `/blog/page/2/`, and so on. Set a site-wide default with
`paginate_by` in `config.yml`.

Inside a page, `page.prev` and `page.next` follow the section's display order: with the default
newest-first sort, `prev` is the newer neighbour and `next` the older one.

## Permalinks

Change where a section's pages live with a pattern:

```yaml
permalinks:
  blog: "/:year/:month/:slug/"
```

Tokens: `:year :month :day` (need a `date`), `:slug`, `:section`, `:title`. The pattern is the
complete URL path. The longest matching section prefix wins.

## Taxonomies

```yaml
taxonomies: [tags, categories]
```

`tags: [a, b]` and `categories: [x]` in front matter then produce `/tags/`, `/tags/a/`,
`/categories/x/`, rendered with the theme's `taxonomy.html` and `term.html`. Any
front-matter list can be a taxonomy, not just these two.

## Menus

```yaml
menus:
  main:
    - { name: Home, url: /, weight: 1 }
    - { name: Blog, url: /blog/, weight: 2 }
```

Themes read them from `config.menus.main`.

## Data files

Any `data/*.yml` is available to templates as `data.<file name>`: `data/people.yml` becomes
`data.people`.
