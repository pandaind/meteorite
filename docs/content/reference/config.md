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
  highlight_theme: InspiredGitHub          # code colours in light mode
  highlight_theme_dark: base16-ocean.dark  # code colours in dark mode
  highlight_style: classes                 # classes (generates highlight.css) | inline

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

Unknown keys do not fail the build, but each one is reported as a warning with a suggestion
(`unknown key `baseurl` (did you mean `base_url`?)`), so typos don't go unnoticed. YAML syntax
errors point at the file, line and column; template errors point at the template line.

## Theme parameters

A theme declares the `params` it understands in its `theme.yml`, with defaults and required
flags. Site values under `params:` override the defaults. If a required parameter has no value
the build stops and names it.
