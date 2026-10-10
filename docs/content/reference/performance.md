---
title: Performance
weight: 4
description: Measured build times and how to reproduce them.
---
Numbers from a 20-thread Linux machine, release build, cold output directory. Each page has
front matter, three tags, a heading, a highlighted Rust block and three paragraphs; pages are
spread over 20 paginated sections with tags enabled.

| Site | Wall time | Notes |
|---|---|---|
| 1,000 pages | 0.20 to 0.23 s | |
| 10,000 pages | 0.9 to 1.1 s | about 47 MB of output |
| 10,000 pages, 1 thread | 3.9 s | `RAYON_NUM_THREADS=1` |
| 10,000 pages, no highlighting | 0.76 s | `markdown.highlight: false` |

Syntax highlighting is the largest single cost. Rendering is parallel across pages.

While `meteorite serve` is running, rebuilds reuse rendered Markdown for files that did not
change and only rewrite output files whose bytes differ; on the starter site that is about
40 to 60 ms.

These figures are for Meteorite alone. They have not been compared against other generators.

## Reproduce

```sh
cargo build --release
scripts/gen-bench-site.sh /tmp/bench 10000 target/release/meteorite
time target/release/meteorite build -r /tmp/bench
```
