//! End-to-end tests for themes: the bundled themes, skins, updating, `theme add` and the starters.

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
