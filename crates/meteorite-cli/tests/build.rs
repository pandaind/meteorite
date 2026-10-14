//! End-to-end tests for building a site: output, front matter, errors, `check`, `new` and `clean`.

#![allow(unused_imports)]
mod common;
use common::*;

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

#[test]
fn golden_output_matches() {
    let site = fixture_copy("golden");
    ok(&site, &["build"]);
    let actual = tree(&site.join("public"));
    let expected_dir = fixtures().join("blog.expected");

    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        let _ = fs::remove_dir_all(&expected_dir);
        for (rel, bytes) in &actual {
            let p = expected_dir.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, bytes).unwrap();
        }
        return;
    }

    let expected = tree(&expected_dir);
    let a: Vec<_> = actual.keys().collect();
    let e: Vec<_> = expected.keys().collect();
    assert_eq!(
        a, e,
        "set of output files differs (run with UPDATE_GOLDEN=1 to refresh)"
    );
    for (rel, bytes) in &expected {
        assert!(
            &actual[rel] == bytes,
            "public/{rel} differs from golden file (run with UPDATE_GOLDEN=1 to refresh)\n--- actual ---\n{}",
            String::from_utf8_lossy(&actual[rel])
        );
    }
}

#[test]
fn build_is_deterministic() {
    let site = fixture_copy("determinism");
    let (a, b) = (site.join("out-a"), site.join("out-b"));
    ok(&site, &["build", "--output", a.to_str().unwrap()]);
    ok(&site, &["build", "--output", b.to_str().unwrap()]);
    assert_eq!(tree(&a), tree(&b));
}

#[test]
fn fixture_semantics() {
    let site = fixture_copy("semantics");
    ok(&site, &["build"]);
    let out = site.join("public");

    // Drafts and future pages are hidden; a bundle publishes its asset beside the page.
    assert!(!out.join("draft").exists());
    assert!(!out.join("future").exists());
    assert_eq!(
        read(&site, "docs/guide/diagram.txt").trim(),
        "diagram-bytes"
    );
    assert_eq!(read(&site, "hello.txt").trim(), "static file");

    // Permalink pattern for the news section.
    assert!(out.join("2026/05/launch/index.html").is_file());

    // Pagination: 4 posts, 2 per page.
    let page1 = read(&site, "blog/index.html");
    assert!(page1.contains("page 1 of 2") && page1.contains("next=/blog/page/2/"));
    let page2 = read(&site, "blog/page/2/index.html");
    assert!(page2.contains("page 2 of 2") && page2.contains("prev=/blog/"));
    // Newest first.
    assert!(page1.find("Delta").unwrap() < page1.find("Gamma").unwrap());

    // Taxonomies.
    let rust = read(&site, "tags/rust/index.html");
    assert!(rust.contains("Alpha") && rust.contains("Beta") && !rust.contains("Gamma"));
    assert!(read(&site, "categories/index.html").contains("life"));

    // TOC with de-duplicated ids, anchors and highlighting.
    let about = read(&site, "about/index.html");
    assert!(about.contains("href=\"#sub-heading-1\""), "{about}");
    assert!(about.contains("<h3 id=\"third-level\">"));
    assert!(about.contains("<pre class=\"hl\">") && about.contains("class=\"hl-"));
    assert!(
        !about.contains("style=\""),
        "classes mode must not emit inline styles"
    );
    let css = read(&site, "highlight.css");
    assert!(
        css.contains(":root[data-theme=\"dark\"] .hl-")
            && css.contains("prefers-color-scheme: dark")
    );

    // Shortcode (paired, with args and rendered Markdown body), summary and prev/next.
    let a = read(&site, "blog/a/index.html");
    assert!(
        a.contains("<aside class=\"note kind-warn\"><p>Careful with <strong>Alpha</strong>.</p>")
    );
    assert!(a.contains("<p class=\"summary\"><p>Intro for Alpha.</p></p>"));
    // Alpha is the oldest of four, so it only has a `prev` (the newer neighbour).
    assert!(a.contains("rel=\"prev\"") && !a.contains("rel=\"next\""));
    let b = read(&site, "blog/b/index.html");
    assert!(b.contains("rel=\"prev\"") && b.contains("rel=\"next\""));

    // Data files and params (theme default merged in).
    let home = read(&site, "index.html");
    assert!(home.contains("<li>Ada</li>") && home.contains("(red)"));

    // Generated files.
    let sitemap = read(&site, "sitemap.xml");
    assert!(sitemap.contains("https://fixture.test/blog/a/"));
    assert!(!sitemap.contains("secret") && !sitemap.contains("future"));
    assert!(read(&site, "atom.xml").contains("<title>Alpha</title>"));
    assert!(read(&site, "rss.xml").contains("<title>Delta</title>"));
    assert!(read(&site, "robots.txt").contains("Sitemap: https://fixture.test/sitemap.xml"));
    let search = read(&site, "search-index.json");
    assert!(search.contains("\"title\":\"Alpha\""));
    assert!(
        !search.contains("Heading One #"),
        "anchor glyphs must not leak into search text"
    );
    assert!(
        search.contains("Careful with Alpha."),
        "inline tags must not split words"
    );
    assert!(out.join("404.html").is_file());
}

#[test]
fn highlight_inline_style_and_unknown_theme() {
    let site = fixture_copy("hl");
    let cfg = fs::read_to_string(site.join("config.yml")).unwrap();

    fs::write(
        site.join("config.yml"),
        format!("{cfg}markdown:\n  highlight_style: inline\n"),
    )
    .unwrap();
    ok(&site, &["build"]);
    let about = read(&site, "about/index.html");
    assert!(about.contains("style=\"") && !about.contains("class=\"hl-"));

    fs::write(
        site.join("config.yml"),
        format!("{cfg}markdown:\n  highlight_theme_dark: not-a-theme\n"),
    )
    .unwrap();
    let out = run(&site, &["build"]);
    assert!(!out.status.success());
    let err = flat(&String::from_utf8_lossy(&out.stderr));
    assert!(
        err.contains("unknown highlight theme `not-a-theme`") && err.contains("base16-ocean.dark"),
        "{err}"
    );
}

#[test]
fn template_syntax_error_points_at_the_line() {
    let site = fixture_copy("tplsyntax");
    fs::create_dir_all(site.join("templates")).unwrap();
    fs::write(
        site.join("templates/page.html"),
        "{% extends \"base.html\" %}\n{% block content %}\n<h1>{{ page.meta.title | }}</h1>\n{% endblock content %}\n",
    )
    .unwrap();
    let out = run(&site, &["build"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    // Match on file name and line only: separators differ on Windows and long paths wrap.
    assert!(squash(&err).contains("page.html:3"), "{err}");
    assert!(flat(&err).contains("template syntax error"), "{err}");
}

#[test]
fn template_render_error_names_the_variable_and_line() {
    let site = fixture_copy("tplrender");
    fs::create_dir_all(site.join("templates")).unwrap();
    fs::write(
        site.join("templates/page.html"),
        "{% extends \"base.html\" %}\n{% block content %}\n<p>ok</p>\n<p>{{ no_such_variable }}</p>\n{% endblock content %}\n",
    )
    .unwrap();
    let out = run(&site, &["build"]);
    assert!(!out.status.success());
    let err = flat(&String::from_utf8_lossy(&out.stderr));
    assert!(err.contains("no_such_variable"), "{err}");
    assert!(squash(&err).contains("page.html:4"), "{err}");
}

#[test]
fn unknown_config_keys_are_reported() {
    let site = fixture_copy("cfgtypo");
    let cfg = fs::read_to_string(site.join("config.yml")).unwrap();
    fs::write(
        site.join("config.yml"),
        format!("baseurl: https://x.test/\n{cfg}"),
    )
    .unwrap();
    let out = run(&site, &["build"]);
    assert!(out.status.success(), "a typo must not fail the build");
    let err = flat(&String::from_utf8_lossy(&out.stderr));
    assert!(
        err.contains("unknown key `baseurl` (did you mean `base_url`?)"),
        "{err}"
    );
}

#[test]
fn drafts_and_future_flags() {
    let site = fixture_copy("flags");
    ok(&site, &["build", "--drafts", "--future"]);
    assert!(site.join("public/draft/index.html").is_file());
    assert!(site.join("public/future/index.html").is_file());
}

#[test]
fn minify_option() {
    let site = fixture_copy("minify");
    ok(&site, &["build", "--minify"]);
    let html = read(&site, "about/index.html");
    assert!(!html.contains("\n\n") && !html.contains("<!--"), "{html}");
    assert!(read(&site, "test.css").contains("body{color:red}"));
}

#[test]
fn missing_required_theme_param_is_an_error() {
    let site = fixture_copy("params");
    let cfg = fs::read_to_string(site.join("config.yml")).unwrap();
    fs::write(
        site.join("config.yml"),
        cfg.replace("  tagline: A site for golden tests\n", ""),
    )
    .unwrap();
    let out = run(&site, &["build"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("tagline"), "{err}");
}

#[test]
fn bad_front_matter_reports_file_and_position() {
    let site = fixture_copy("yaml");
    fs::write(site.join("content/about.md"), "---\ntitle: [oops\n---\nx\n").unwrap();
    let out = run(&site, &["build"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("about.md:2"), "{err}");
}

#[test]
fn unsafe_slug_is_rejected() {
    let site = fixture_copy("slug");
    fs::write(
        site.join("content/evil.md"),
        "---\ntitle: x\nslug: ../../escape\n---\n",
    )
    .unwrap();
    let out = run(&site, &["build"]);
    assert!(!out.status.success());
    assert!(!site.parent().unwrap().join("escape").exists());
}

#[test]
fn check_finds_broken_links() {
    let site = fixture_copy("check");
    assert!(run(&site, &["check"]).status.success());
    fs::write(
        site.join("content/links.md"),
        "---\ntitle: Links\n---\n[gone](/does/not/exist/) and [fine](/about/#heading-one) and [ext](https://example.org)\n",
    )
    .unwrap();
    let out = run(&site, &["check"]);
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("/does/not/exist/") && text.contains("1 problem"),
        "{text}"
    );
}

#[test]
fn new_page_command_is_safe() {
    let dir = tmp("newpage");
    let site = dir.join("s");
    ok(&dir, &["new", "site", site.to_str().unwrap()]);
    ok(&site, &["new", "page", "notes/idea", "--title", "An idea"]);
    assert!(site.join("content/notes/idea.md").is_file());
    assert!(!run(&site, &["new", "page", "../escape"]).status.success());
    assert!(!run(&site, &["new", "page", "notes/idea"]).status.success()); // exists
    assert!(!dir.join("escape.md").exists());
}

#[test]
fn build_never_wipes_a_foreign_directory() {
    let site = fixture_copy("foreign");
    let precious = tmp("precious");
    fs::write(precious.join("thesis.docx"), "do not delete").unwrap();

    let out = run(&site, &["build", "--output", precious.to_str().unwrap()]);
    assert!(!out.status.success());
    assert!(flat(&String::from_utf8_lossy(&out.stderr)).contains("not created by Meteorite"));
    assert!(precious.join("thesis.docx").is_file());

    // An empty directory, and a previous Meteorite output, are both fine.
    let empty = tmp("empty-out");
    ok(&site, &["build", "--output", empty.to_str().unwrap()]);
    ok(&site, &["build", "--output", empty.to_str().unwrap()]);
    assert!(empty.join("index.html").is_file());
}

#[test]
fn clean_removes_public() {
    let site = fixture_copy("clean");
    ok(&site, &["build"]);
    assert!(site.join("public").is_dir());
    ok(&site, &["clean"]);
    assert!(!site.join("public").exists());
}
