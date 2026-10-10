# Vendored: PopTalk chat widget

`poptalk.min.js` is the built widget from https://github.com/pandaind/poptalk
(`widget/dist/poptalk.min.js`), MIT licensed (see `LICENSE`).

| | |
|---|---|
| Version | 1.1.0 (tag `v1.1.0`) |
| SHA-384 | `so1XZhQ2oUFVggJKW7w+RzKzvqZJwVjy0tcc56LZe7BpcPRKiVo6GdxbN8vtJSIy` |

The file is byte-identical to what poptalk's CI builds from that tag (Node 20, `npm ci`), and to
what jsDelivr serves at `@v1.1.0`.

Meteorite copies it into a site's output only when `chat.provider: poptalk` is configured and
`chat.widget` is `bundled` (the default). Refresh it by copying the new build here, updating the
table above, and bumping `WIDGET_VERSION` in `src/chat.rs`.
