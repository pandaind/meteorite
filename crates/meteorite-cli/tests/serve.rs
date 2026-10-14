//! End-to-end tests for `meteorite serve`.

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
