---
title: config.yml
weight: 2
---
Every key is optional.

```yaml
title: My Site                  # shown by themes as config.title
base_url: https://example.com/  # absolute for feeds/sitemap; may include a sub-path
language: en
theme: default                  # directory name under themes/

params:                         # free-form; themes read config.params.<key>
  author: Me

menus:
  main:
    - { name: Home, url: /, weight: 1 }

taxonomies: [tags]
permalinks:
  blog: "/:year/:month/:slug/"
paginate_by: 10                 # default page size for section listings

markdown:
  highlight: true
  highlight_theme: InspiredGitHub

build:
  drafts: false
  future: false
  minify: false

feed:
  enabled: true
  limit: 20
sitemap: true
search:
  enabled: false
```

Unknown keys are ignored, so a typo does not fail the build. YAML syntax errors point at the
file, line and column.

## Theme parameters

A theme declares the `params` it understands in its `theme.yml`, with defaults and required
flags. Site values under `params:` override the defaults. If a required parameter has no value
the build stops and names it.
