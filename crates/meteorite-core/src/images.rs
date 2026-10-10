//! `resize_image` template function (feature `images`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tera::{Tera, Value};

/// Paths (relative to the output dir) of files generated while rendering.
pub type Generated = Arc<Mutex<Vec<PathBuf>>>;

#[cfg(feature = "images")]
pub fn register(
    tera: &mut Tera,
    root: &Path,
    out_dir: &Path,
    base_url: &str,
    generated: Generated,
) {
    let root = root.to_path_buf();
    let out_dir = out_dir.to_path_buf();
    let base_url = base_url.to_string();
    tera.register_function("resize_image", move |args: &HashMap<String, Value>| {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| tera::Error::msg("resize_image(): `path` is required"))?;
        let width = args
            .get("width")
            .and_then(Value::as_u64)
            .filter(|w| (1..=10_000).contains(w))
            .ok_or_else(|| tera::Error::msg("resize_image(): `width` must be 1..=10000"))?
            as u32;
        if Path::new(path).components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        }) {
            return Err(tera::Error::msg(format!(
                "resize_image(): unsafe path `{path}`"
            )));
        }
        let src = ["static", "content"]
            .iter()
            .map(|d| root.join(d).join(path))
            .find(|p| p.is_file())
            .ok_or_else(|| {
                tera::Error::msg(format!(
                    "resize_image(): `{path}` not found in static/ or content/"
                ))
            })?;
        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
        let ext = src.extension().and_then(|s| s.to_str()).unwrap_or("png");
        let rel = PathBuf::from("processed").join(format!("{stem}-{width}w.{ext}"));
        let dest = out_dir.join(&rel);
        if !dest.exists() {
            let img =
                image::open(&src).map_err(|e| tera::Error::msg(format!("resize_image(): {e}")))?;
            let resized = if img.width() <= width {
                img
            } else {
                img.resize(width, u32::MAX, image::imageops::FilterType::Lanczos3)
            };
            std::fs::create_dir_all(dest.parent().unwrap())
                .map_err(|e| tera::Error::msg(e.to_string()))?;
            resized
                .save(&dest)
                .map_err(|e| tera::Error::msg(format!("resize_image(): {e}")))?;
        }
        generated.lock().unwrap().push(rel.clone());
        Ok(Value::String(crate::util::join_url(
            &base_url,
            &rel.to_string_lossy().replace('\\', "/"),
        )))
    });
}

#[cfg(not(feature = "images"))]
pub fn register(
    tera: &mut Tera,
    _root: &Path,
    _out_dir: &Path,
    _base_url: &str,
    _generated: Generated,
) {
    tera.register_function("resize_image", |_: &HashMap<String, Value>| {
        Err(tera::Error::msg(
            "resize_image() needs a build with `--features images` (cargo install meteorite --features images)",
        ))
    });
}
