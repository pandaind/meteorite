use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Lowercase, alphanumeric-and-dash slug. Never contains `/`, `.` or whitespace.
pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            dash = false;
        } else if !out.is_empty() && !dash {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// Join a base URL (`/`, `/sub/`, `https://x.com/`) and a path with exactly one slash.
pub fn join_url(base: &str, path: &str) -> String {
    if path.contains("://") || path.starts_with("//") {
        return path.to_string();
    }
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

pub fn is_absolute_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Escape for HTML text and attribute values. Unlike Tera's default this leaves `/`
/// alone so URLs stay readable in themes.
pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Tags that separate words visually; everything else (`<strong>`, `<a>`, ...) is dropped
/// without inserting a space, so "a<b>b</b>c" stays "abc".
fn is_block_tag(tag: &str) -> bool {
    let name = tag
        .trim_start_matches('/')
        .split(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "p" | "br"
            | "div"
            | "li"
            | "ul"
            | "ol"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "tr"
            | "td"
            | "th"
            | "pre"
            | "blockquote"
            | "section"
            | "article"
            | "aside"
            | "hr"
            | "table"
            | "figure"
            | "figcaption"
    )
}

/// Remove heading anchor links (`<a class="anchor" ...>#</a>`) that Meteorite adds.
pub fn strip_anchors(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find("<a class=\"anchor\"") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</a>") {
            Some(end) => rest = &rest[start + end + 4..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Remove HTML tags and collapse whitespace. Good enough for search snippets.
pub fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut chars = html.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '<' {
            let end = html[i..].find('>').map(|e| i + e).unwrap_or(html.len());
            if is_block_tag(&html[i + 1..end.max(i + 1)]) {
                out.push(' ');
            }
            while let Some(&(j, _)) = chars.peek() {
                if j <= end {
                    chars.next();
                } else {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    let out = out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn hash_parts(parts: &[&str]) -> u64 {
    let mut h = DefaultHasher::new();
    for p in parts {
        p.hash(&mut h);
        0u8.hash(&mut h);
    }
    h.finish()
}

/// `my-first_post` -> `My first post`.
pub fn humanize(stem: &str) -> String {
    let s = stem.replace(['-', '_'], " ");
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("  --a__b "), "a-b");
        assert_eq!(slugify("../etc/passwd"), "etc-passwd");
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn joins() {
        assert_eq!(join_url("/", "/a/"), "/a/");
        assert_eq!(join_url("https://x.com/sub/", "a/"), "https://x.com/sub/a/");
        assert_eq!(
            join_url("https://x.com", "https://y.com/z"),
            "https://y.com/z"
        );
    }

    #[test]
    fn tags() {
        assert_eq!(
            strip_tags("<p>Hi <b>there</b> &amp; you</p>"),
            "Hi there & you"
        );
        assert_eq!(strip_tags("<p>a<b>b</b>c</p><p>d</p>"), "abc d");
        // Unterminated `<` followed by multi-byte text must not panic.
        assert_eq!(strip_tags("x < é“"), "x");
        assert_eq!(strip_tags("日本語 <b>太字</b>"), "日本語 太字");
        assert_eq!(
            strip_tags(&strip_anchors(
                "<h2 id=\"x\">T <a class=\"anchor\" href=\"#x\">#</a></h2>"
            )),
            "T"
        );
    }
}
