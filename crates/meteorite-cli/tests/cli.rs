//! End-to-end tests: drive the real `meteorite` binary against fixture sites.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_meteorite");

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("meteorite-it-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dest = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &dest);
        } else {
            fs::copy(e.path(), dest).unwrap();
        }
    }
}

fn fixture_copy(name: &str) -> PathBuf {
    let dir = tmp(name);
    copy_dir(&fixtures().join("blog"), &dir);
    dir
}

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

fn ok(root: &Path, args: &[&str]) -> String {
    let out = run(root, args);
    assert!(
        out.status.success(),
        "`meteorite {}` failed:\n{}\n{}",
        args.join(" "),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(base, &p, out);
            } else if p.file_name().is_some_and(|n| n == ".meteorite-output") {
                // Bookkeeping marker, not part of the site.
            } else {
                let rel = p
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, fs::read(&p).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

/// Diagnostics are word-wrapped to the terminal width, which differs between machines.
/// Collapse the wrapping so assertions can match phrases.
fn flat(s: &str) -> String {
    s.replace(['│', '╭', '╰', '─'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Like `flat`, but with all whitespace removed. Long paths get word-wrapped mid-path, so
/// assertions about "file:line" must ignore the wrapping entirely.
fn squash(s: &str) -> String {
    flat(s).split_whitespace().collect()
}

fn read(root: &Path, rel: &str) -> String {
    fs::read_to_string(root.join("public").join(rel))
        .unwrap_or_else(|e| panic!("cannot read public/{rel}: {e}"))
}

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
fn bundled_themes_ship_a_color_mode_toggle() {
    let dir = tmp("toggle");
    let site = dir.join("s");
    ok(&dir, &["new", "site", site.to_str().unwrap()]);
    let page = || read(&site, "index.html");
    ok(&site, &["build"]);
    let html = page();
    assert!(html.contains("id=\"theme-toggle\"") && html.contains("meteorite-theme"));
    assert!(html.contains("highlight.css"));
    // color_scheme pins the starting mode; theme_toggle: false removes the button.
    let cfg = fs::read_to_string(site.join("config.yml")).unwrap();
    fs::write(
        site.join("config.yml"),
        cfg.replace(
            "  author: Me\n",
            "  author: Me\n  color_scheme: dark\n  theme_toggle: false\n",
        ),
    )
    .unwrap();
    ok(&site, &["build"]);
    let html = page();
    let tag = &html[html.find("<html").unwrap()..];
    let tag = &tag[..tag.find('>').unwrap()];
    assert!(tag.contains("data-theme=\"dark\""), "{tag}");
    assert!(!html.contains("id=\"theme-toggle\""));
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
fn bundled_themes_render_a_search_dialog_when_enabled() {
    let dir = tmp("search");
    let site = dir.join("s");
    ok(&dir, &["new", "site", site.to_str().unwrap()]);
    ok(&site, &["theme", "add", "terminal"]);
    for theme in ["default", "terminal"] {
        let cfg = fs::read_to_string(site.join("config.yml")).unwrap();
        let cfg: String = cfg
            .lines()
            .map(|l| {
                if l.starts_with("theme:") {
                    format!("theme: {theme}")
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(site.join("config.yml"), &cfg).unwrap();
        ok(&site, &["build"]);
        let home = read(&site, "index.html");
        assert!(
            home.contains("id=\"search-dialog\""),
            "{theme}: dialog missing"
        );
        assert!(
            home.contains("data-index=\"https://example.com/search-index.json\""),
            "{theme}"
        );
        assert!(home.contains("id=\"search-open\""), "{theme}");
        assert!(
            read(&site, "search-index.json").contains("Hello, world"),
            "{theme}"
        );
        assert!(
            read(&site, "app.js").contains("search-dialog"),
            "{theme}: app.js lacks search"
        );

        // Disabled: no dialog, no button, no index.
        fs::write(
            site.join("config.yml"),
            cfg.replace("enabled: true", "enabled: false"),
        )
        .unwrap();
        ok(&site, &["build"]);
        assert!(
            !read(&site, "index.html").contains("search-dialog"),
            "{theme}"
        );
        assert!(!site.join("public/search-index.json").exists(), "{theme}");
        fs::write(site.join("config.yml"), &cfg).unwrap();
    }
}

#[test]
fn bundled_themes_ship_a_favicon_that_the_site_can_override_or_drop() {
    let dir = tmp("favicon");
    let site = dir.join("s");
    ok(&dir, &["new", "site", site.to_str().unwrap()]);
    ok(&site, &["theme", "add", "terminal"]);

    for theme in ["default", "terminal"] {
        ok(&site, &["build", "--theme", theme]);
        let home = read(&site, "index.html");
        assert!(
            home.contains("rel=\"icon\"")
                && home.contains("favicon.svg")
                && home.contains("image/svg+xml"),
            "{theme}: icon link missing"
        );
        let svg = read(&site, "favicon.svg");
        assert!(
            svg.starts_with("<svg") && svg.contains("viewBox"),
            "{theme}: not an SVG"
        );
        // The link resolves, so `check` stays green.
        assert!(
            run(&site, &["check", "--theme", theme]).status.success(),
            "{theme}"
        );
    }

    // A favicon.svg in the site's own static/ wins over the theme's.
    fs::write(site.join("static/favicon.svg"), "<svg id=\"mine\"/>").unwrap();
    ok(&site, &["build", "--theme", "default"]);
    assert_eq!(read(&site, "favicon.svg"), "<svg id=\"mine\"/>");

    // A different file, in another format, via the parameter.
    fs::write(site.join("static/icon.png"), b"\x89PNG").unwrap();
    let cfg = fs::read_to_string(site.join("config.yml")).unwrap();
    fs::write(
        site.join("config.yml"),
        cfg.replace("  author: Me\n", "  author: Me\n  favicon: icon.png\n"),
    )
    .unwrap();
    ok(&site, &["build", "--theme", "default"]);
    let home = read(&site, "index.html");
    assert!(home.contains("icon.png") && !home.contains("image/svg+xml"));

    // Empty string means "no icon link".
    let cfg = fs::read_to_string(site.join("config.yml")).unwrap();
    fs::write(
        site.join("config.yml"),
        cfg.replace("favicon: icon.png", "favicon: \"\""),
    )
    .unwrap();
    ok(&site, &["build", "--theme", "default"]);
    assert!(!read(&site, "index.html").contains("rel=\"icon\""));
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
fn new_site_builds_with_every_bundled_theme_and_same_pages() {
    let dir = tmp("themes");
    let site = dir.join("s");
    ok(&dir, &["new", "site", site.to_str().unwrap()]);
    ok(&site, &["theme", "add", "terminal"]);
    assert!(ok(&site, &["theme", "list"]).contains("terminal"));

    let pages = |theme: &str| -> Vec<String> {
        let cfg = fs::read_to_string(site.join("config.yml")).unwrap();
        let cfg = cfg
            .lines()
            .map(|l| {
                if l.starts_with("theme:") {
                    format!("theme: {theme}")
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(site.join("config.yml"), cfg).unwrap();
        ok(&site, &["build"]);
        assert!(
            run(&site, &["check"]).status.success(),
            "check failed for {theme}"
        );
        tree(&site.join("public"))
            .into_keys()
            .filter(|k| k.ends_with(".html"))
            .collect()
    };
    let a = pages("default");
    let b = pages("terminal");
    assert_eq!(
        a, b,
        "same content must yield the same pages under any theme"
    );
    assert!(a.contains(&"blog/hello-world/index.html".to_string()));

    // The active theme can't be removed; the inactive one can.
    assert!(!run(&site, &["theme", "remove", "terminal"])
        .status
        .success());
    ok(&site, &["theme", "remove", "default"]);
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
fn theme_add_from_local_directory() {
    let site = fixture_copy("themeadd");
    let src = site.join("themes/test");
    let out = run(
        &site,
        &["theme", "add", src.to_str().unwrap(), "--name", "copy"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(site.join("themes/copy/theme.yml").is_file());
    // Invalid names are refused.
    assert!(!run(
        &site,
        &["theme", "add", src.to_str().unwrap(), "--name", "../x"]
    )
    .status
    .success());
}

#[test]
fn site_templates_and_static_override_theme() {
    let site = fixture_copy("override");
    fs::create_dir_all(site.join("templates")).unwrap();
    fs::write(
        site.join("templates/page.html"),
        "{% extends \"base.html\" %}{% block content %}SITE-OVERRIDE {{ page.meta.title }}{% endblock content %}",
    )
    .unwrap();
    // Site static beats theme static of the same name.
    fs::write(site.join("static/test.css"), "/* site css */").unwrap();
    ok(&site, &["build"]);
    assert!(read(&site, "about/index.html").contains("SITE-OVERRIDE About"));
    // Templates the site didn't override still come from the theme.
    assert!(read(&site, "blog/index.html").contains("page 1 of 2"));
    assert_eq!(read(&site, "test.css"), "/* site css */");
}

#[cfg(unix)]
#[test]
fn theme_add_from_git_url() {
    if Command::new("git").arg("--version").output().is_err() {
        eprintln!("git not available; skipping");
        return;
    }
    let dir = tmp("gittheme");
    let repo = dir.join("remote-theme");
    copy_dir(&fixtures().join("blog/themes/test"), &repo);
    let git = |args: &[&str]| {
        let s = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(["-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"])
            .args(args)
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(
            s.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&s.stderr)
        );
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "theme"]);

    let site = fixture_copy("gitsite");
    fs::remove_dir_all(site.join("themes/test")).unwrap();
    let url = format!("file://{}", repo.display());
    let out = run(&site, &["theme", "add", &url, "--name", "fromgit"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(site.join("themes/fromgit/theme.yml").is_file());
    assert!(
        !site.join("themes/fromgit/.git").exists(),
        ".git must be stripped"
    );

    // A repo that is not a theme is rejected and leaves nothing behind.
    fs::remove_file(repo.join("theme.yml")).unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "break"]);
    let out = run(&site, &["theme", "add", &url, "--name", "broken"]);
    assert!(!out.status.success());
    assert!(!site.join("themes/broken").exists());
    let leftovers: Vec<_> = fs::read_dir(site.join("themes"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(".staging"))
        .collect();
    assert!(leftovers.is_empty(), "staging dir must be cleaned up");
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

/// A copy of the fixture's theme under another name, with a marker in its base template.
fn add_marked_theme(site: &Path, name: &str, marker: &str) {
    let src = site.join("themes/test");
    let out = run(
        site,
        &["theme", "add", src.to_str().unwrap(), "--name", name],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let base = site.join("themes").join(name).join("templates/base.html");
    let html = fs::read_to_string(&base).unwrap();
    fs::write(&base, format!("<!-- {marker} -->\n{html}")).unwrap();
}

#[test]
fn theme_flag_overrides_config_without_editing_it() {
    let site = fixture_copy("themeflag");
    add_marked_theme(&site, "alt", "ALT-THEME-MARKER");
    let before = fs::read_to_string(site.join("config.yml")).unwrap();

    ok(&site, &["build"]);
    assert!(!read(&site, "about/index.html").contains("ALT-THEME-MARKER"));

    ok(&site, &["build", "--theme", "alt"]);
    assert!(read(&site, "about/index.html").contains("ALT-THEME-MARKER"));
    assert_eq!(fs::read_to_string(site.join("config.yml")).unwrap(), before);

    // check honours it too, and bad names are refused before anything is touched.
    assert!(run(&site, &["check", "--theme", "alt"]).status.success());
    for bad in ["../x", "a/b", "nope"] {
        let out = run(&site, &["build", "--theme", bad]);
        assert!(!out.status.success(), "{bad}");
    }
}

#[test]
fn bundled_themes_offer_skins_and_a_style_menu() {
    let dir = tmp("skins");
    let site = dir.join("s");
    ok(&dir, &["new", "site", site.to_str().unwrap()]);
    ok(&site, &["theme", "add", "terminal"]);
    for (theme, default_skin, others) in [
        ("default", "meteor", ["paper", "aurora"]),
        ("terminal", "green", ["amber", "mono"]),
    ] {
        ok(&site, &["build", "--theme", theme]);
        let home = read(&site, "index.html");
        assert!(
            home.contains(&format!("data-skin=\"{default_skin}\"")),
            "{theme}"
        );
        assert!(
            home.contains("class=\"skin-menu\""),
            "{theme}: style menu missing"
        );
        for skin in others {
            assert!(
                home.contains(&format!("data-skin-set=\"{skin}\"")),
                "{theme}: {skin}"
            );
            // Every skin has CSS, so choosing it can never produce an unstyled page.
            let css = ["style.css", "terminal.css"]
                .iter()
                .find_map(|f| fs::read_to_string(site.join("public").join(f)).ok())
                .unwrap();
            assert!(
                css.contains(&format!("[data-skin=\"{skin}\"]")),
                "{theme}: no CSS for {skin}"
            );
        }
        // The visitor's saved choice is applied before paint, from a validated allow-list.
        assert!(
            home.contains("meteorite-skin") && home.contains(&format!("\"{default_skin}\"")),
            "{theme}"
        );
    }

    // A configured skin is pinned on <html>; the menu can be switched off.
    let cfg = fs::read_to_string(site.join("config.yml")).unwrap();
    fs::write(
        site.join("config.yml"),
        cfg.replace(
            "  author: Me\n",
            "  author: Me\n  skin: paper\n  skin_switcher: false\n",
        ),
    )
    .unwrap();
    ok(&site, &["build", "--theme", "default"]);
    let home = read(&site, "index.html");
    assert!(home.contains("data-skin=\"paper\"") && !home.contains("class=\"skin-menu\""));
}

#[test]
fn stale_bundled_theme_is_detected_and_updated_with_a_backup() {
    let dir = tmp("update");
    let site = dir.join("s");
    ok(&dir, &["new", "site", site.to_str().unwrap()]);

    // Make the installed copy look like an older release with a local customisation.
    let theme_yml = site.join("themes/default/theme.yml");
    let yml = fs::read_to_string(&theme_yml).unwrap();
    let old = yml
        .lines()
        .map(|l| {
            if l.starts_with("version:") {
                "version: 0.1.0"
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&theme_yml, old).unwrap();
    fs::write(
        site.join("themes/default/static/MY-CUSTOM.css"),
        "/* mine */",
    )
    .unwrap();

    let list = ok(&site, &["theme", "list"]);
    assert!(list.contains("update available"), "{list}");

    // Building says so too, instead of silently rendering without the newer features.
    let stale_build = run(&site, &["build"]);
    assert!(stale_build.status.success());
    let warn = flat(&String::from_utf8_lossy(&stale_build.stderr));
    assert!(warn.contains("meteorite theme update default"), "{warn}");

    let out = ok(&site, &["theme", "update"]);
    assert!(out.contains("Updated `default` 0.1.0 ->"), "{out}");
    // Fresh version installed, and the old copy (with the customisation) is kept.
    assert!(!fs::read_to_string(&theme_yml)
        .unwrap()
        .contains("version: 0.1.0"));
    assert!(!site.join("themes/default/static/MY-CUSTOM.css").exists());
    let backups: Vec<_> = fs::read_dir(site.join("themes/.backup"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(backups.len(), 1);
    assert!(backups[0].path().join("static/MY-CUSTOM.css").is_file());

    // The site builds with the refreshed theme, and now reports nothing to update.
    let fresh = run(&site, &["build"]);
    assert!(!String::from_utf8_lossy(&fresh.stderr).contains("theme update"));
    assert!(read(&site, "index.html").contains("class=\"skin-menu\""));
    assert!(ok(&site, &["theme", "list"])
        .find("update available")
        .is_none());
    assert!(ok(&site, &["theme", "update"]).contains("up to date"));
    // Backups are not themes: they must not show up in `theme list`.
    assert!(!ok(&site, &["theme", "list"]).contains(".backup"));

    // Only bundled themes can be updated this way.
    let custom = site.join("themes/mine");
    copy_dir(&site.join("themes/default"), &custom);
    fs::write(custom.join("theme.yml"), "name: mine\nversion: 1.0.0\n").unwrap();
    let out = run(&site, &["theme", "update", "mine"]);
    assert!(!out.status.success());
    assert!(flat(&String::from_utf8_lossy(&out.stderr)).contains("not a bundled theme"));
}

#[test]
fn unknown_skin_is_a_warning_not_an_error() {
    let dir = tmp("skinwarn");
    let site = dir.join("s");
    ok(&dir, &["new", "site", site.to_str().unwrap()]);
    let cfg = fs::read_to_string(site.join("config.yml")).unwrap();
    fs::write(
        site.join("config.yml"),
        cfg.replace("  author: Me\n", "  author: Me\n  skin: neon-pink\n"),
    )
    .unwrap();
    let out = run(&site, &["build"]);
    assert!(out.status.success());
    let err = flat(&String::from_utf8_lossy(&out.stderr));
    assert!(
        err.contains("`neon-pink` is not a skin of theme `default`"),
        "{err}"
    );
    assert!(err.contains("meteor"), "lists the available skins: {err}");
}

// ---- chat (PopTalk) -------------------------------------------------------------

fn with_chat(site: &Path, yaml: &str) {
    let cfg = fs::read_to_string(site.join("config.yml")).unwrap();
    fs::write(site.join("config.yml"), format!("{cfg}\nchat:\n{yaml}")).unwrap();
}

#[test]
fn chat_is_off_by_default_and_adds_nothing() {
    let site = fixture_copy("chatoff");
    ok(&site, &["build"]);
    let home = read(&site, "index.html");
    assert!(!home.contains("data-mt-chat") && !home.contains("poptalk"));
    assert!(!tree(&site.join("public"))
        .keys()
        .any(|k| k.contains("poptalk")));
    // The command explains how to turn it on instead of failing obscurely.
    let out = run(&site, &["chat", "check"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stdout).contains("provider: poptalk"));
}

#[test]
fn chat_adds_a_launcher_and_a_self_hosted_widget_with_matching_integrity() {
    use base64::Engine;
    use sha2::{Digest, Sha384};
    let site = fixture_copy("chaton");
    // The fixture's own theme has no hook; add the one line a theme needs.
    let base = site.join("themes/test/templates/base.html");
    let html = fs::read_to_string(&base).unwrap();
    fs::write(&base, format!("{html}\n{{{{ chat_widget() | safe }}}}\n")).unwrap();
    with_chat(
        &site,
        "  provider: poptalk\n  api_url: https://chat.example.com/\n  persona: docs\n  label: Ask us\n",
    );
    ok(&site, &["build"]);
    let home = read(&site, "index.html");
    assert!(home.contains("data-mt-chat") && home.contains("Ask us"));
    assert!(
        home.contains("\"api\":\"https://chat.example.com\"")
            && home.contains("\"persona\":\"docs\"")
    );

    // The widget is served from the site itself, so there is no third-party request.
    let widget = tree(&site.join("public"))
        .into_iter()
        .find(|(k, _)| k.starts_with("assets/poptalk-") && k.ends_with(".js"))
        .expect("bundled widget copied into the site");
    assert!(home.contains(&format!("https://fixture.test/{}", widget.0)));
    for third_party in ["jsdelivr", "googleapis", "unpkg", "cdnjs"] {
        assert!(
            !home.contains(third_party),
            "{third_party} must not appear in the page"
        );
    }
    // Integrity in the page matches the bytes actually shipped.
    let sri = base64::engine::general_purpose::STANDARD.encode(Sha384::digest(&widget.1));
    assert!(
        home.contains(&format!("sha384-{sri}")),
        "integrity hash mismatch"
    );
    assert!(
        site.join("public/assets/poptalk-LICENSE.txt").is_file(),
        "licence must ship with the widget"
    );
}

#[test]
fn chat_enabled_but_theme_has_no_hook_is_a_warning() {
    let site = fixture_copy("chatnohook"); // the fixture theme does not call chat_widget()
    with_chat(
        &site,
        "  provider: poptalk\n  api_url: https://chat.example.com\n",
    );
    let out = run(&site, &["build"]);
    assert!(out.status.success());
    let err = flat(&String::from_utf8_lossy(&out.stderr));
    assert!(err.contains("never calls chat_widget()"), "{err}");
    assert!(!read(&site, "index.html").contains("data-mt-chat"));
}

#[test]
fn chat_config_rejects_secrets_and_bad_values() {
    for (name, yaml, needle) in [
        ("secret", "  provider: poptalk\n  api_url: https://c.test\n  api_key: sk-123\n", "looks like a credential"),
        ("token", "  provider: poptalk\n  api_url: https://c.test\n  knowledge:\n    token: abc\n", "looks like a credential"),
        ("url", "  provider: poptalk\n  api_url: javascript:alert(1)\n", "api_url"),
        ("provider", "  provider: acme\n  api_url: https://c.test\n", "provider"),
        ("keyenv", "  provider: poptalk\n  api_url: https://c.test\n  knowledge:\n    key_env: sk-live-abc123\n", "NAME of an environment variable"),
        ("accent", "  provider: poptalk\n  api_url: https://c.test\n  accent: 'red; }</style><script>x'\n", "accent"),
    ] {
        let site = fixture_copy(&format!("chatbad-{name}"));
        with_chat(&site, yaml);
        let out = run(&site, &["build"]);
        assert!(!out.status.success(), "{name} must be rejected");
        let err = flat(&String::from_utf8_lossy(&out.stderr));
        assert!(err.contains(needle), "{name}: {err}");
    }
}

#[test]
fn knowledge_export_writes_clean_markdown_and_only_touches_its_own_files() {
    let site = fixture_copy("knowledge");
    let kdir = tmp("knowledge-out");
    // Someone else's document already lives in the knowledge folder.
    fs::write(kdir.join("handbook.pdf"), "not ours").unwrap();
    fs::write(
        site.join("content/_index.md"),
        "---\ntitle: Home\nhero:\n  title: Fixture hero headline\n  subtitle: \"A sentence, with a comma.\"\nfeatures:\n  title: Why\n  items:\n    - { title: Plain files, text: \"No database, no runtime.\" }\n---\nWelcome.\n",
    )
    .unwrap();
    fs::write(
        site.join("content/blog/private.md"),
        "---\ntitle: Private\nchat: false\ndate: 2026-01-01\n---\nsecret roadmap\n",
    )
    .unwrap();
    with_chat(
        &site,
        &format!(
            "  knowledge:\n    dir: {}\n    exclude: [news]\n",
            kdir.display()
        ),
    );

    let out = ok(&site, &["build"]);
    assert!(out.contains("Knowledge export:"), "{out}");
    let read_k =
        |rel: &str| fs::read_to_string(kdir.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));

    // Front matter carries what is needed to cite the page.
    let post = read_k("blog/a.md");
    assert!(post.starts_with("---\n"));
    for want in [
        "title: Alpha",
        "url: https://fixture.test/blog/a/",
        "section: blog",
        "date: 2026-01-01",
        "tags:",
    ] {
        assert!(post.contains(want), "missing {want:?}:\n{post}");
    }
    // Shortcodes become their text, not template syntax or HTML.
    assert!(
        post.contains("Careful with **Alpha**")
            && !post.contains("{{<")
            && !post.contains("<aside")
    );
    // Home-page blocks become searchable prose.
    let home = read_k("index.md");
    assert!(
        home.contains("## Fixture hero headline") && home.contains("A sentence, with a comma.")
    );
    assert!(home.contains("### Plain files") && home.contains("No database, no runtime."));

    // Drafts, future pages, opted-out pages and excluded sections are not exported.
    for absent in [
        "draft.md",
        "future.md",
        "blog/private.md",
        "news/index.md",
        "2026/05/launch.md",
    ] {
        assert!(!kdir.join(absent).exists(), "{absent} must not be exported");
    }
    assert!(!fs::read_to_string(kdir.join(".meteorite-export"))
        .unwrap()
        .contains("private"));
    // The foreign file is untouched.
    assert_eq!(
        fs::read_to_string(kdir.join("handbook.pdf")).unwrap(),
        "not ours"
    );

    // Re-running changes nothing; removing a page removes only its exported file.
    assert!(ok(&site, &["build"]).contains("0 written"));
    fs::remove_file(site.join("content/blog/b.md")).unwrap();
    let out = ok(&site, &["build"]);
    assert!(out.contains("1 removed"), "{out}");
    assert!(!kdir.join("blog/b.md").exists() && kdir.join("blog/a.md").is_file());
    assert!(kdir.join("handbook.pdf").is_file());

    // --no-export leaves the folder alone.
    fs::remove_file(site.join("content/blog/c.md")).unwrap();
    ok(&site, &["build", "--no-export"]);
    assert!(kdir.join("blog/c.md").is_file());
}

#[test]
fn knowledge_export_refuses_unsafe_targets() {
    // A file Meteorite did not create, at a path it wants to write.
    let site = fixture_copy("knowledge-conflict");
    let kdir = tmp("knowledge-conflict-out");
    fs::create_dir_all(kdir.join("blog")).unwrap();
    fs::write(kdir.join("blog/a.md"), "hand-written, do not replace").unwrap();
    with_chat(
        &site,
        &format!("  knowledge:\n    dir: {}\n", kdir.display()),
    );
    let out = run(&site, &["build"]);
    assert!(!out.status.success());
    assert!(flat(&String::from_utf8_lossy(&out.stderr)).contains("refusing to overwrite"));
    assert_eq!(
        fs::read_to_string(kdir.join("blog/a.md")).unwrap(),
        "hand-written, do not replace"
    );

    // Targets inside the site's own source or output directories.
    for inside in ["content", "themes", "public", "."] {
        let site = fixture_copy(&format!("knowledge-inside-{}", inside.trim_matches('.')));
        with_chat(&site, &format!("  knowledge:\n    dir: {inside}\n"));
        let out = run(&site, &["build"]);
        assert!(!out.status.success(), "{inside} must be refused");
        assert!(
            site.join("config.yml").is_file(),
            "site source must be intact"
        );
    }
}

#[test]
fn knowledge_can_be_published_as_one_deterministic_json_file() {
    use sha2::{Digest, Sha256};
    let site = fixture_copy("publish");
    with_chat(
        &site,
        "  knowledge:\n    publish: true\n    exclude: [news]\n",
    );
    ok(&site, &["build"]);
    let first = read(&site, "knowledge.json");
    let v: serde_json::Value = serde_json::from_str(&first).expect("valid JSON");

    assert_eq!(v["version"], 1);
    assert!(v["generator"].as_str().unwrap().starts_with("meteorite "));
    assert_eq!(v["site"], "https://fixture.test/");
    let docs = v["documents"].as_array().unwrap();
    let paths: Vec<&str> = docs.iter().map(|d| d["path"].as_str().unwrap()).collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted, "documents are ordered by path");
    assert!(paths.contains(&"blog/a.md") && paths.contains(&"index.md"));
    for absent in ["draft.md", "future.md", "news/index.md"] {
        assert!(!paths.contains(&absent), "{absent} must not be published");
    }

    // Each document carries what is needed to cite it, and a hash of exactly its content.
    let a = docs.iter().find(|d| d["path"] == "blog/a.md").unwrap();
    assert_eq!(a["title"], "Alpha");
    assert_eq!(a["url"], "https://fixture.test/blog/a/");
    assert_eq!(a["section"], "blog");
    assert_eq!(a["date"], "2026-01-01");
    assert!(a["tags"].as_array().unwrap().iter().any(|t| t == "rust"));
    let content = a["content"].as_str().unwrap();
    assert!(!content.starts_with("---"), "content has no front matter");
    assert!(content.starts_with("# Alpha") && content.contains("Careful with **Alpha**"));
    let want = format!(
        "sha256:{}",
        Sha256::digest(content.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    assert_eq!(a["hash"], want);

    // Identical content gives a byte-identical file; editing a page changes the digest.
    ok(&site, &["build"]);
    assert_eq!(
        read(&site, "knowledge.json"),
        first,
        "no timestamps or ordering noise"
    );
    fs::write(
        site.join("content/about.md"),
        "---\ntitle: About\n---\nChanged.\n",
    )
    .unwrap();
    ok(&site, &["build"]);
    let v2: serde_json::Value = serde_json::from_str(&read(&site, "knowledge.json")).unwrap();
    assert_ne!(v2["digest"], v["digest"]);
    let unchanged = |name: &str, a: &serde_json::Value, b: &serde_json::Value| {
        let find = |v: &serde_json::Value| {
            v["documents"]
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["path"] == name)
                .unwrap()["hash"]
                .clone()
        };
        assert_eq!(find(a), find(b), "{name} did not change");
    };
    unchanged("blog/a.md", &v, &v2);
}

#[test]
fn knowledge_is_not_published_unless_asked_and_the_path_is_validated() {
    let site = fixture_copy("publish-off");
    ok(&site, &["build"]);
    assert!(!site.join("public/knowledge.json").exists());

    for (name, path) in [
        ("up", "../x.json"),
        ("abs", "/x.json"),
        ("ext", "notes.txt"),
        ("back", "a\\b.json"),
    ] {
        let site = fixture_copy(&format!("publish-bad-{name}"));
        with_chat(
            &site,
            &format!("  knowledge:\n    publish: true\n    publish_path: '{path}'\n"),
        );
        let out = run(&site, &["build"]);
        assert!(!out.status.success(), "{path} must be rejected");
        assert!(
            flat(&String::from_utf8_lossy(&out.stderr)).contains("publish_path"),
            "{path}"
        );
    }

    // A custom location inside the output works.
    let site = fixture_copy("publish-custom");
    with_chat(
        &site,
        "  knowledge:\n    publish: true\n    publish_path: data/kb.json\n",
    );
    ok(&site, &["build"]);
    assert!(site.join("public/data/kb.json").is_file());
}

/// A one-connection-at-a-time HTTP server that records raw requests.
fn fake_backend(
    status: u16,
    headers: &'static str,
    body: &'static str,
) -> (u16, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            log.lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buf[..n]).into_owned());
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (port, seen)
}

#[test]
fn chat_sync_posts_a_reindex_with_the_key_from_the_environment() {
    let site = fixture_copy("chatsync");
    let (port, seen) = fake_backend(
        200,
        "Content-Type: application/json\r\n",
        "{\"status\":\"ok\"}",
    );
    with_chat(
        &site,
        &format!("  provider: poptalk\n  api_url: https://c.test\n  knowledge:\n    rag_url: http://127.0.0.1:{port}/\n"),
    );

    // No key in the environment: a clear error, and no request is made.
    let out = Command::new(BIN)
        .arg("--root")
        .arg(&site)
        .args(["chat", "sync"])
        .env_remove("POPTALK_RAG_KEY")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(flat(&String::from_utf8_lossy(&out.stderr)).contains("POPTALK_RAG_KEY is not set"));
    assert!(seen.lock().unwrap().is_empty());

    let out = Command::new(BIN)
        .arg("--root")
        .arg(&site)
        .args(["chat", "sync"])
        .env("POPTALK_RAG_KEY", "persona-key-123")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let req = seen.lock().unwrap()[0].clone();
    assert!(req.starts_with("POST /admin/reindex HTTP/1.1"), "{req}");
    assert!(
        req.to_lowercase()
            .contains("authorization: bearer persona-key-123"),
        "{req}"
    );
    // The key is never echoed back or written anywhere in the site.
    assert!(!String::from_utf8_lossy(&out.stdout).contains("persona-key-123"));
    assert!(!fs::read_to_string(site.join("config.yml"))
        .unwrap()
        .contains("persona-key-123"));
}

#[test]
fn chat_sync_reports_a_rejected_key() {
    let site = fixture_copy("chatsync401");
    let (port, _) = fake_backend(401, "", "");
    with_chat(
        &site,
        &format!("  provider: poptalk\n  api_url: https://c.test\n  knowledge:\n    rag_url: http://127.0.0.1:{port}\n"),
    );
    let out = Command::new(BIN)
        .arg("--root")
        .arg(&site)
        .args(["chat", "sync"])
        .env("POPTALK_RAG_KEY", "wrong")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(flat(&String::from_utf8_lossy(&out.stderr)).contains("rejected the key"));
}

#[test]
fn chat_check_verifies_the_backend_and_cors() {
    let body = "{\"ownerName\":\"Acme\",\"chatTitle\":\"Ask Acme\",\"chatMode\":\"AI\"}";
    let http_site = |name: &str| {
        let site = fixture_copy(name);
        let cfg = fs::read_to_string(site.join("config.yml")).unwrap();
        fs::write(
            site.join("config.yml"),
            cfg.replace("https://fixture.test/", "http://fixture.test/"),
        )
        .unwrap();
        site
    };

    // Allowed origin: success.
    let site = http_site("chatcheck-ok");
    let (port, seen) = fake_backend(
        200,
        "Content-Type: application/json\r\nAccess-Control-Allow-Origin: http://fixture.test\r\n",
        body,
    );
    with_chat(
        &site,
        &format!("  provider: poptalk\n  api_url: http://127.0.0.1:{port}\n  persona: acme\n"),
    );
    let out = run(&site, &["chat", "check"]);
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "{text}");
    assert!(
        text.contains("Ask Acme") && text.contains("CORS allows http://fixture.test"),
        "{text}"
    );
    let req = seen.lock().unwrap()[0].clone();
    assert!(req.starts_with("GET /api/config?persona=acme"), "{req}");
    assert!(
        req.contains("rigin: http://fixture.test"),
        "sends the site's origin: {req}"
    );

    // Origin not allowed: a precise, actionable failure.
    let site = http_site("chatcheck-cors");
    let (port, _) = fake_backend(
        200,
        "Content-Type: application/json\r\nAccess-Control-Allow-Origin: https://other.test\r\n",
        body,
    );
    with_chat(
        &site,
        &format!("  provider: poptalk\n  api_url: http://127.0.0.1:{port}\n"),
    );
    let out = run(&site, &["chat", "check"]);
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text.contains("CORS_ALLOWED_ORIGIN=http://fixture.test"),
        "{text}"
    );

    // An https site must not be paired with an http backend (browsers block mixed content).
    let site = fixture_copy("chatcheck-mixed"); // base_url is https://fixture.test/
    let (port, _) = fake_backend(
        200,
        "Content-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n",
        body,
    );
    with_chat(
        &site,
        &format!("  provider: poptalk\n  api_url: http://127.0.0.1:{port}\n"),
    );
    let out = run(&site, &["chat", "check"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stdout).contains("mixed content"));
}

// ---- dev server --------------------------------------------------------------

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn try_get_with(port: u16, path: &str, headers: &[&str]) -> std::io::Result<(u16, String)> {
    let mut s = TcpStream::connect(("127.0.0.1", port))?;
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    let extra: String = headers.iter().map(|h| format!("{h}\r\n")).collect();
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n{extra}\r\n"
    )?;
    let mut raw = Vec::new();
    s.read_to_end(&mut raw)?;
    let buf = String::from_utf8_lossy(&raw).into_owned();
    let status = buf
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| std::io::Error::other(format!("malformed response: {buf:?}")))?;
    Ok((status, buf))
}

/// Socket-level hiccups on a loaded machine are retried; HTTP-level results are not.
fn get(port: u16, path: &str) -> (u16, String) {
    get_with(port, path, &[])
}

fn get_with(port: u16, path: &str, headers: &[&str]) -> (u16, String) {
    let mut last = None;
    for _ in 0..20 {
        match try_get_with(port, path, headers) {
            Ok(r) => return r,
            Err(e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
    panic!("GET {path} failed after retries: {:?}", last);
}

#[test]
fn serve_serves_reloads_and_blocks_traversal() {
    let site = fixture_copy("serve");
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let child = Command::new(BIN)
        .arg("--root")
        .arg(&site)
        .args(["serve", "--port", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _server = Server(child);

    let deadline = Instant::now() + Duration::from_secs(20);
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(Instant::now() < deadline, "server did not start");
        std::thread::sleep(Duration::from_millis(100));
    }

    let (status, body) = get(port, "/about/");
    assert_eq!(status, 200);
    assert!(
        body.contains("/__meteorite/ws"),
        "live reload script missing"
    );
    // base_url is overridden so links stay local while serving.
    assert!(body.contains(&format!("http://127.0.0.1:{port}/")));
    assert_eq!(get(port, "/does-not-exist/").0, 404);
    assert_eq!(get(port, "/../../etc/passwd").0, 400);
    assert_eq!(get(port, "/%2e%2e/%2e%2e/etc/passwd").0, 400);

    // Editing content triggers a rebuild.
    fs::write(
        site.join("content/about.md"),
        "---\ntitle: About\n---\nLIVE EDIT MARKER\n",
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if get(port, "/about/").1.contains("LIVE EDIT MARKER") {
            break;
        }
        assert!(Instant::now() < deadline, "edit was not picked up");
        std::thread::sleep(Duration::from_millis(150));
    }
}

#[test]
fn serve_previews_other_themes_without_touching_config() {
    let site = fixture_copy("serveswitch");
    add_marked_theme(&site, "alt", "ALT-THEME-MARKER");
    let config_before = fs::read_to_string(site.join("config.yml")).unwrap();
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let child = Command::new(BIN)
        .arg("--root")
        .arg(&site)
        .args(["serve", "--port", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _server = Server(child);
    let deadline = Instant::now() + Duration::from_secs(20);
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(Instant::now() < deadline, "server did not start");
        std::thread::sleep(Duration::from_millis(100));
    }

    // The toolbar lists installed themes and marks the active one; no marker yet.
    let (_, page) = get(port, "/about/");
    assert!(page.contains("data-meteorite-dev"), "toolbar missing");
    assert!(
        page.contains("\"alt\"") && page.contains("\"test\""),
        "themes not listed"
    );
    assert!(!page.contains("ALT-THEME-MARKER"));

    // Choosing a theme builds it, sets a cookie, and redirects back.
    let (status, resp) = get(port, "/__meteorite/theme?name=alt&next=/about/");
    assert_eq!(status, 302, "{resp}");
    assert!(
        resp.contains("location: /about/") || resp.contains("Location: /about/"),
        "{resp}"
    );
    assert!(
        resp.to_lowercase()
            .contains("set-cookie: meteorite-dev-theme=alt"),
        "{resp}"
    );

    // With the cookie the alternate theme is served from the same URLs; without it, the original.
    let (_, themed) = get_with(port, "/about/", &["Cookie: meteorite-dev-theme=alt"]);
    assert!(themed.contains("ALT-THEME-MARKER"), "{themed}");
    let (_, plain) = get(port, "/about/");
    assert!(!plain.contains("ALT-THEME-MARKER"));
    // A cookie naming a theme that was never built falls back safely.
    let (_, odd) = get_with(port, "/about/", &["Cookie: meteorite-dev-theme=../../etc"]);
    assert!(odd.contains("Fixture Site") && !odd.contains("ALT-THEME-MARKER"));

    // Bad input is refused; open redirects are neutralised.
    assert_eq!(get(port, "/__meteorite/theme?name=../x").0, 400);
    assert_eq!(get(port, "/__meteorite/theme?name=nope").0, 400);
    assert_eq!(get(port, "/__meteorite/theme").0, 400);
    let (s2, r2) = get(port, "/__meteorite/theme?name=alt&next=//evil.example/x");
    assert_eq!(s2, 302);
    assert!(
        r2.contains("ocation: /\r\n"),
        "redirect must fall back to /: {r2}"
    );

    // Edits rebuild the alternate theme too.
    fs::write(
        site.join("content/about.md"),
        "---\ntitle: About\n---\nSWITCH-EDIT\n",
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let (_, p) = get_with(port, "/about/", &["Cookie: meteorite-dev-theme=alt"]);
        if p.contains("SWITCH-EDIT") && p.contains("ALT-THEME-MARKER") {
            break;
        }
        assert!(Instant::now() < deadline, "alternate theme was not rebuilt");
        std::thread::sleep(Duration::from_millis(150));
    }
    assert_eq!(
        fs::read_to_string(site.join("config.yml")).unwrap(),
        config_before
    );
}

#[test]
fn orbit_starter_builds_a_personal_site() {
    let dir = tmp("orbit");
    let site = dir.join("s");
    ok(
        &dir,
        &["new", "site", site.to_str().unwrap(), "--theme", "orbit"],
    );
    ok(&site, &["build"]);
    ok(&site, &["check"]);

    // Home: profile, a project, a post and a note, plus structured data for search engines.
    let home = read(&site, "index.html");
    assert!(home.contains("hero-title") && home.contains("Selected work"));
    assert!(home.contains("application/ld+json") && home.contains("\"@type\": \"Person\""));
    assert!(home.contains("First project") && home.contains("Hello, world"));
    assert!(
        home.contains("note-item"),
        "recent notes appear on the home page"
    );

    // Micro-blog: the whole note is in the listing, and untitled notes show no heading.
    let notes = read(&site, "notes/index.html");
    assert!(notes.contains("Write one in a minute"));
    assert!(!notes.contains("note-title"));

    // Resume and links pages render from data files and front matter.
    let resume = read(&site, "resume/index.html");
    assert!(resume.contains("Senior Engineer") && resume.contains("data-print"));
    let links = read(&site, "links/index.html");
    assert!(links.contains("link-btn") && links.contains("GitHub"));

    // A project page uses the project layout.
    let project = read(&site, "projects/first-project/index.html");
    assert!(project.contains("project-facts") && project.contains("Rust"));
}

#[test]
fn devlog_starter_builds_and_shares_content_with_orbit() {
    let dir = tmp("devlog");
    let site = dir.join("s");
    ok(
        &dir,
        &["new", "site", site.to_str().unwrap(), "--theme", "devlog"],
    );
    ok(&site, &["build"]);
    ok(&site, &["check"]);

    let home = read(&site, "index.html");
    assert!(home.contains("README.md") && home.contains("repo-card"));
    assert!(
        home.contains("id=\"activity-data\""),
        "activity graph data is embedded"
    );
    assert!(home.contains("lang-dot"), "stack entries get language dots");
    assert!(
        home.contains("note-win"),
        "recent notes appear on the home page"
    );
    assert!(read(&site, "notes/index.html").contains("Write one in a minute"));
    assert!(read(&site, "resume/index.html").contains("Senior Engineer"));
    assert!(read(&site, "projects/first-project/index.html").contains("project-facts"));

    // The same content renders under the sibling theme: switch themes and build again.
    ok(&site, &["theme", "add", "orbit"]);
    let cfg = fs::read_to_string(site.join("config.yml")).unwrap();
    fs::write(
        site.join("config.yml"),
        cfg.replace("theme: devlog", "theme: orbit"),
    )
    .unwrap();
    ok(&site, &["build"]);
    assert!(read(&site, "index.html").contains("hero-title"));
}
