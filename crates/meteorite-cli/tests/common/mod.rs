//! Helpers shared by the end-to-end test files: fixture sites, temp folders and running the
//! real `meteorite` binary.
#![allow(dead_code, unused_imports)]

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

pub const BIN: &str = env!("CARGO_BIN_EXE_meteorite");

pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

pub fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("meteorite-it-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

pub fn copy_dir(from: &Path, to: &Path) {
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

pub fn fixture_copy(name: &str) -> PathBuf {
    let dir = tmp(name);
    copy_dir(&fixtures().join("blog"), &dir);
    dir
}

pub fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

pub fn ok(root: &Path, args: &[&str]) -> String {
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

pub fn tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
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
pub fn flat(s: &str) -> String {
    s.replace(['│', '╭', '╰', '─'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Like `flat`, but with all whitespace removed. Long paths get word-wrapped mid-path, so
/// assertions about "file:line" must ignore the wrapping entirely.
pub fn squash(s: &str) -> String {
    flat(s).split_whitespace().collect()
}

pub fn read(root: &Path, rel: &str) -> String {
    fs::read_to_string(root.join("public").join(rel))
        .unwrap_or_else(|e| panic!("cannot read public/{rel}: {e}"))
}

/// A copy of the fixture's theme under another name, with a marker in its base template.
pub fn add_marked_theme(site: &Path, name: &str, marker: &str) {
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
