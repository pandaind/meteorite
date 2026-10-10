//! A default avatar for people who have no picture: a symmetric 5x5 pattern in one colour, derived
//! from a seed (usually the person's name), in the style of GitHub's identicons. Deterministic, so
//! the same name always draws the same picture, and rendered as inline SVG so it needs no files.

use sha2::{Digest, Sha256};

use crate::util::html_escape;

/// Inline SVG for `seed`. The background has the class `identicon-bg` so a theme can colour it.
pub fn svg(seed: &str) -> String {
    let digest = Sha256::digest(seed.trim().to_lowercase().as_bytes());
    let hue = (u16::from(digest[0]) << 8 | u16::from(digest[1])) % 360;
    // 15 bits decide the left three columns of the 5x5 grid; the right two mirror columns 1 and 0.
    let mut bits = (u32::from(digest[2]) << 8 | u32::from(digest[3])) & 0x7fff;
    if bits == 0 {
        bits = 0b000_010_000_010_000; // never draw an empty square
    }
    let mut cells = String::new();
    for row in 0..5u32 {
        for col in 0..3u32 {
            if bits >> (row * 3 + col) & 1 == 1 {
                for x in if col == 2 {
                    vec![2]
                } else {
                    vec![col, 4 - col]
                } {
                    cells.push_str(&format!(
                        "<rect x=\"{}\" y=\"{}\" width=\"1\" height=\"1\"/>",
                        x + 1,
                        row + 1
                    ));
                }
            }
        }
    }
    format!(
        "<svg class=\"identicon\" viewBox=\"0 0 7 7\" role=\"img\" aria-label=\"Avatar for {}\" \
shape-rendering=\"crispEdges\"><rect class=\"identicon-bg\" width=\"7\" height=\"7\"/>\
<g fill=\"hsl({hue} 55% 52%)\">{cells}</g></svg>",
        html_escape(seed.trim())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_draws_the_same_picture() {
        assert_eq!(svg("Ada Lovelace"), svg("Ada Lovelace"));
        assert_eq!(svg("Ada Lovelace"), svg("  Ada Lovelace "));
        // Case does not change the pattern, only the accessible label.
        let pattern = |s: &str| s[s.find("<g ").unwrap()..].to_string();
        assert_eq!(pattern(&svg("Ada Lovelace")), pattern(&svg("ada lovelace")));
    }

    #[test]
    fn different_seeds_differ() {
        assert_ne!(svg("Ada Lovelace"), svg("Grace Hopper"));
    }

    #[test]
    fn the_pattern_is_mirrored_and_never_empty() {
        for seed in ["a", "b", "Chittaranjan Panda", "", "x y z"] {
            let out = svg(seed);
            let xs: Vec<(u32, u32)> = out
                .split("<rect x=\"")
                .skip(1) // the text before the first cell
                .map(|r| {
                    let x = r.split('"').next().unwrap().parse().unwrap();
                    let y = r.split("y=\"").nth(1).unwrap().split('"').next().unwrap();
                    (x, y.parse().unwrap())
                })
                .collect();
            assert!(!xs.is_empty(), "empty pattern for {seed:?}");
            for (x, y) in &xs {
                assert!(
                    xs.contains(&(6 - x, *y)),
                    "{seed:?}: ({x},{y}) has no mirror"
                );
            }
        }
    }

    #[test]
    fn the_seed_cannot_inject_markup() {
        let out = svg("<script>alert(1)</script>");
        assert!(!out.contains("<script>"));
    }
}
