# Meteorite

A static site generator in Rust, in the spirit of Hugo: **Markdown** content, **YAML**
configuration, **drop-in themes**, and no database. One binary, one folder of files.

```sh
cargo install meteorite
meteorite new site mysite && cd mysite
meteorite serve            # http://127.0.0.1:1111, live reload
```

## Features

- Markdown with YAML front matter, sections, bundles, drafts, future/expiry dates
- Themes are plain folders; switch with `theme: name`. Two bundled, third-party via `theme add <dir|git-url>`
- Site `templates/` and `static/` override the theme without forking it
- Pagination, taxonomies, menus, permalinks, data files, table of contents, heading anchors
- Build-time syntax highlighting, shortcodes, optional image resizing
- Atom + RSS feeds, sitemap, robots.txt, client-side search index
- `meteorite check` finds broken internal links; clear errors with file, line and column
- Incremental rebuilds while serving; about 1 s for 10,000 pages on a 20-thread machine

## Documentation

The docs are a Meteorite site in [`docs/`](docs/):

```sh
meteorite serve -r docs
```

Start with `docs/content/guide/getting-started.md`, then the
[theme authoring guide](docs/content/themes/authoring.md) and the
[security model](docs/content/reference/security.md).

## Layout of this repository

| Path | |
|---|---|
| `crates/meteorite-core` | The library: config, content, site model, rendering, themes |
| `crates/meteorite-cli` | The `meteorite` binary: commands and the dev server |
| `crates/meteorite-core/assets/themes` | Bundled themes |
| `docs/` | Documentation site |
| `scripts/gen-bench-site.sh` | Synthetic site generator for benchmarks |
| `PLAN.md` | Roadmap and design decisions |

## Development

```sh
cargo test --workspace
cargo test -p meteorite-core --features images     # image resizing
cargo clippy --all-targets -- -D warnings
UPDATE_GOLDEN=1 cargo test -p meteorite --test cli golden   # refresh golden output; review the diff
```

Requires Rust 1.88 or newer (the minimum is set by dependencies such as `time`).

## Licence

MIT or Apache-2.0, at your option.
