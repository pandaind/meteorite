use std::fs;
use std::path::{Path, PathBuf};

use meteorite_core::build::{BuildOptions, Builder};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../meteorite-cli/tests/fixtures/blog")
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

#[test]
fn rebuild_touches_only_what_changed() {
    let dir = std::env::temp_dir().join(format!("meteorite-inc-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    copy_dir(&fixture(), &dir);

    let mut b = Builder::new(&dir);
    let opts = BuildOptions::default();
    let first = b.build(&opts).unwrap();
    assert!(first.files_written > 10);

    // Nothing changed: nothing written.
    let again = b.build(&opts).unwrap();
    assert_eq!(again.files_written, 0, "{again:?}");
    assert_eq!(again.files_unchanged, first.files_written);

    // Edit one post: only that page (and listings that quote it) are rewritten.
    let post = dir.join("content/blog/a.md");
    let text = fs::read_to_string(&post).unwrap();
    fs::write(
        &post,
        text.replace("Intro for Alpha.", "Intro for Alpha, edited."),
    )
    .unwrap();
    let third = b.build(&opts).unwrap();
    assert!(
        third.files_written >= 1 && third.files_written < first.files_written / 2,
        "{third:?}"
    );
    assert!(fs::read_to_string(dir.join("public/blog/a/index.html"))
        .unwrap()
        .contains("edited"));

    // Delete a page: its output disappears, as does its now-empty directory.
    fs::remove_file(dir.join("content/blog/b.md")).unwrap();
    let fourth = b.build(&opts).unwrap();
    assert!(fourth.files_removed >= 1);
    assert!(!dir.join("public/blog/b").exists());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn refuses_to_wipe_the_site_root() {
    let dir = std::env::temp_dir().join(format!("meteorite-wipe-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    copy_dir(&fixture(), &dir);
    let opts = BuildOptions {
        out_dir: Some(dir.clone()),
        ..Default::default()
    };
    assert!(meteorite_core::build::build(&dir, &opts).is_err());
    assert!(dir.join("config.yml").is_file(), "site source must survive");
    let _ = fs::remove_dir_all(&dir);
}
