# Vendored: PopTalk chat widget

`poptalk.min.js` is the built widget from https://github.com/pandaind/poptalk
(`widget/dist/poptalk.min.js`, version 1.1.0), MIT licensed (see `LICENSE`).

Meteorite copies it into a site's output only when `chat.provider: poptalk` is configured
and `chat.widget` is `bundled` (the default). Refresh it by copying the new build here and
bumping `WIDGET_VERSION` in `src/chat.rs`.
