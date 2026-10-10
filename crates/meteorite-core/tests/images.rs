//! Image resizing is behind the `images` feature; without it the template function
//! must fail with a clear message rather than silently doing nothing.

use std::fs;
use std::path::{Path, PathBuf};

use meteorite_core::build::{build, BuildOptions};

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

fn site(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("meteorite-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    copy_dir(&fixture(), &dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(
        dir.join("templates/page.html"),
        "{% extends \"base.html\" %}{% block content %}<img src=\"{{ resize_image(path='pic.png', width=40) }}\">{% endblock content %}",
    )
    .unwrap();
    dir
}

#[cfg(feature = "images")]
#[test]
fn resizes_images_and_keeps_them_across_rebuilds() {
    let dir = site("img");
    let mut png = image::RgbImage::new(200, 100);
    for p in png.pixels_mut() {
        *p = image::Rgb([200, 30, 30]);
    }
    fs::create_dir_all(dir.join("static")).unwrap();
    png.save(dir.join("static/pic.png")).unwrap();

    let mut b = meteorite_core::build::Builder::new(&dir);
    b.build(&BuildOptions::default()).unwrap();
    let out = dir.join("public/processed/pic-40w.png");
    let img = image::open(&out).expect("resized image written");
    assert_eq!((img.width(), img.height()), (40, 20));
    let html = fs::read_to_string(dir.join("public/about/index.html")).unwrap();
    assert!(html.contains("https://fixture.test/processed/pic-40w.png"));

    // A second build must not delete the generated file as "stale".
    b.build(&BuildOptions::default()).unwrap();
    assert!(out.is_file());

    // Path traversal is refused.
    fs::write(
        dir.join("templates/page.html"),
        "{{ resize_image(path='../config.yml', width=10) }}",
    )
    .unwrap();
    let err = b.build(&BuildOptions::default()).unwrap_err().to_string();
    assert!(err.contains("unsafe path"), "{err}");
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(not(feature = "images"))]
#[test]
fn explains_how_to_enable_images() {
    let dir = site("noimg");
    let err = build(&dir, &BuildOptions::default())
        .unwrap_err()
        .to_string();
    assert!(err.contains("--features images"), "{err}");
    let _ = fs::remove_dir_all(&dir);
}
