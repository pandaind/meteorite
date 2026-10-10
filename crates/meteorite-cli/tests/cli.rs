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
    assert!(about.contains("style=\"")); // syntect inline styles

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

// ---- dev server --------------------------------------------------------------

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn get(port: u16, path: &str) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut buf = String::new();
    s.read_to_string(&mut buf).unwrap();
    let status = buf.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, buf)
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
