---
title: Command line
weight: 1
---
Every command accepts `-r, --root <DIR>` to operate on a site other than the current directory.

| Command | What it does |
|---|---|
| `meteorite new site <dir> [--theme <name>]` | Scaffold a site with a bundled theme (default: `default`). |
| `meteorite new page <path> [--title <t>]` | Create `content/<path>.md` as a draft. |
| `meteorite build` | Build into `public/`. |
| `meteorite serve` | Build, serve and live-reload. |
| `meteorite check` | Build to a scratch directory and report broken internal links. Exit code 1 on problems. |
| `meteorite clean` | Delete `public/`. |
| `meteorite theme list` | Show installed and bundled themes. `*` marks the active one. |
| `meteorite theme add <src> [--name <n>]` | Install a theme from a bundled name, a directory, or a git URL. |
| `meteorite theme remove <name>` | Delete an installed theme (not the active one). |

## build

| Flag | Meaning |
|---|---|
| `--drafts` | Include `draft: true` pages. |
| `--future` | Include pages dated in the future. |
| `--minify` | Minify HTML and CSS. |
| `--base-url <url>` | Override `base_url`. |
| `-o, --output <dir>` | Write somewhere other than `public/`. |

Meteorite deletes and regenerates the output directory. For safety it refuses to touch a
non-empty directory that it did not create (it leaves a `.meteorite-output` marker file), and
it refuses to use the site root or any of its parents.

## serve

| Flag | Default | Meaning |
|---|---|---|
| `-p, --port` | `1111` | Port. |
| `--bind` | `127.0.0.1` | Address. Use `0.0.0.0` to reach it from another device. |
| `--open` | off | Open a browser. |
| `--drafts`, `--future` | off | As for `build`. |

While serving, `base_url` is replaced with the local address so links stay on your machine.
Only `content/`, `static/`, `templates/`, `themes/`, `data/` and `config.yml` are watched.
