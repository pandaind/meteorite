---
title: Add a chat assistant
weight: 5
description: Optional AI or human chat on every page, with your site as its knowledge base.
---
Meteorite has first-class, **optional** support for [PopTalk](https://github.com/pandaind/poptalk):
a self-hosted chat backend with a small widget, which can answer with an AI model or forward
messages to you, and [poptalk-rag](https://github.com/pandaind/poptalk-rag), which lets it answer
from your own content. Nothing is added to your site unless you turn it on.

A static site cannot run a chatbot by itself, so there are two parts:

| Part | Where it runs | What it does |
|---|---|---|
| **PopTalk backend** (and optionally poptalk-rag) | Your own server | Talks to the AI provider or to you; holds all keys |
| **Meteorite** | Build time | Adds the widget to your pages and exports your content as the knowledge base |

## 1. Turn on the widget

```yaml
# config.yml
chat:
  provider: poptalk
  api_url: https://chat.example.com     # your PopTalk backend
  persona: my-site                      # optional, for multi-persona backends
```

That is all. Both bundled themes already include the hook; a custom theme needs one line before
`</body>`:

```html
{{ chat_widget() | safe }}
```

Check the backend, including that it allows your site's origin (CORS):

```sh
meteorite chat check
```

### What visitors get

- A launcher button. **Nothing is requested from the chat backend until it is clicked**, so
  there are no cookies or network calls before the visitor chooses to chat.
- The widget file is copied into your site (`assets/poptalk-<hash>.js`, with a subresource
  integrity hash and its MIT licence), so no CDN is contacted. Fonts come from the visitor's
  system by default; set `fonts: google` to use Inter from Google Fonts.
- The widget follows your site's light/dark mode and skin accent live.

### Options

| Key | Default | Meaning |
|---|---|---|
| `provider` | none | `poptalk` enables chat |
| `api_url` | none | Backend base URL (required when enabled) |
| `persona` | backend default | Persona id |
| `lang` | visitor's browser | `en`, `fr`, `pt`, ... |
| `theme` | `auto` | `auto` follows the site; or `dark` / `light` |
| `accent` | `auto` | `auto` follows the site accent; or any CSS colour |
| `load` | `click` | `click` (launcher), `idle`, or `immediate` |
| `widget` | `bundled` | `bundled`, or an `https://` URL you manage |
| `fonts` | `system` | `system` or `google` |
| `label` | `Chat` | Launcher text |

## 2. Use your site as the knowledge base (optional)

poptalk-rag indexes documents in `knowledge/<persona>/`. Point Meteorite at that folder and every
`meteorite build` keeps it in sync:

```yaml
chat:
  provider: poptalk
  api_url: https://chat.example.com
  knowledge:
    dir: ../poptalk-rag/knowledge/my-site
    exclude: [drafts]                  # sections to leave out; `include:` limits instead
    rag_url: https://rag.example.com   # only for `meteorite chat sync`
```

Each published page becomes one Markdown file with the front matter needed to cite it:

```yaml
---
title: Getting started
url: https://example.com/guide/getting-started/
section: guide
date: 2026-10-10
tags: [intro]
---
```

What is exported, and what is not:

- Only what you publish: drafts, future-dated and expired pages are left out.
- Shortcodes become their plain text, and the home page's `hero`, `features`, `stats`, `showcase`,
  `quote` and `cta` blocks become prose, so nothing a visitor can read is missing.
- Add `chat: false` to a page's front matter to keep it out.
- Meteorite only ever touches files it created (it keeps a `.meteorite-export` list). Your own
  documents in the same folder are left alone, and it refuses to overwrite a file it didn't write.
- The folder must be outside the site (not `content/`, `themes/`, `public/` or the site root).
- `serve` and `check` never export; `meteorite build --no-export` skips it too.

### Re-index after deploying

poptalk-rag polls its folder, so exported files are picked up on their own. To trigger a pass
immediately, for example from CI:

```sh
export POPTALK_RAG_KEY=...      # the persona's key; never put it in config.yml
meteorite build
meteorite chat sync
```

## Privacy and security

- **Keys never enter the site.** `config.yml` is published with your site, so Meteorite refuses
  any `chat:` key that looks like a credential (`api_key`, `token`, `secret`, ...). The persona
  key is read from the environment variable named by `chat.knowledge.key_env`.
- Messages go to **your** backend and then to the AI provider you configured there. Say so in
  your privacy notice.
- Keep `load: click` unless you have a reason not to; it is what makes "no request before the
  visitor opts in" true.
