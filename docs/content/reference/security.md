---
title: Security model
weight: 3
---
Meteorite treats **content** as trusted (you wrote it) and **themes and shortcodes** as
untrusted-but-sandboxed (templates and static files only).

| Concern | Behaviour |
|---|---|
| Path escape from content | Slugs may not contain `/`, `\`, `:`, `..` or start with `.`; section paths are slugified; permalink patterns that expand to `..` are rejected; every output path is re-checked before writing. |
| Symlinks | Never followed in `content/`, `static/`, `data/` or themes. Symlinks in a cloned theme are removed. |
| Output directory | Only an empty directory or one carrying Meteorite's marker is ever deleted. |
| Dev server | Serves only files under the output directory; `..`, encoded `..`, backslashes and NUL are refused. Binds to `127.0.0.1` by default. |
| `theme add <git-url>` | `git clone --depth 1 -- <url>` with transports limited to https, ssh, git and file; no submodules; no prompts. |
| Dev theme picker | `/__meteorite/theme` accepts only installed theme names (`[A-Za-z0-9_-]`) and redirects only to same-site paths; it exists only under `meteorite serve`. |
| Templates | Tera has no filesystem, network or process access. |
| Raw HTML in Markdown | Passed through unchanged. Don't render untrusted Markdown with Meteorite. |

Report vulnerabilities privately to the maintainers rather than in a public issue.
