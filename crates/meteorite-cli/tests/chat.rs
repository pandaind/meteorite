//! End-to-end tests for the PopTalk chat integration and the knowledge-base export.

#![allow(unused_imports)]
// Tests read top to bottom as a list of checks; the length lints are meant for library code.
#![allow(clippy::too_many_lines, clippy::cognitive_complexity)]
mod common;
use common::*;

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

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
