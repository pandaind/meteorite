//! Conservative HTML/CSS minification: whitespace and comments only, never rewrites tokens.

/// Tags whose contents must be left exactly as written.
const RAW: [&str; 4] = ["pre", "textarea", "script", "style"];

pub fn html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while !rest.is_empty() {
        if let Some(stripped) = rest.strip_prefix("<!--") {
            // Keep conditional comments; drop everything else.
            match stripped.find("-->") {
                Some(end) => {
                    if stripped.starts_with("[if") || stripped.starts_with("<![") {
                        out.push_str(&rest[..4 + end + 3]);
                    }
                    rest = &stripped[end + 3..];
                    continue;
                }
                None => {
                    out.push_str(rest);
                    break;
                }
            }
        }
        if rest.starts_with('<') {
            if let Some(tag) = raw_tag_at(rest) {
                let close = format!("</{tag}");
                let lower = rest.to_ascii_lowercase();
                let end = lower
                    .find(&close)
                    .and_then(|p| lower[p..].find('>').map(|g| p + g + 1))
                    .unwrap_or(rest.len());
                out.push_str(&rest[..end]);
                rest = &rest[end..];
                continue;
            }
        }
        let c = rest.chars().next().unwrap();
        if c.is_whitespace() {
            let ws = rest.len() - rest.trim_start().len();
            if !out.ends_with(' ') && !out.is_empty() {
                out.push(' ');
            }
            rest = &rest[ws..];
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out.trim().to_string()
}

fn raw_tag_at(s: &str) -> Option<&'static str> {
    // Work on bytes: the text after `<` may start with a multi-byte character, and
    // slicing a &str at a byte offset inside it would panic.
    let after = s.as_bytes().get(1..)?;
    RAW.iter().copied().find(|t| {
        after.len() > t.len()
            && after[..t.len()].eq_ignore_ascii_case(t.as_bytes())
            && matches!(after[t.len()], b'>' | b' ' | b'\n' | b'\t' | b'/')
    })
}

pub fn css(input: &str) -> String {
    let mut stripped = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("/*") {
        stripped.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => {
                rest = "";
                break;
            }
        }
    }
    stripped.push_str(rest);

    let mut out = String::with_capacity(stripped.len());
    let mut in_str: Option<char> = None;
    let mut pending_space = false;
    for c in stripped.chars() {
        if let Some(q) = in_str {
            out.push(c);
            if c == q {
                in_str = None;
            }
            continue;
        }
        if c.is_whitespace() {
            pending_space = true;
            continue;
        }
        let tight = matches!(c, '{' | '}' | ';' | ',' | ':' | '>');
        let prev_tight = out
            .chars()
            .last()
            .is_none_or(|p| matches!(p, '{' | '}' | ';' | ',' | ':' | '>'));
        if pending_space && !tight && !prev_tight {
            out.push(' ');
        }
        pending_space = false;
        if c == '"' || c == '\'' {
            in_str = Some(c);
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_collapses_but_keeps_pre() {
        let src = "<div>\n   <p>a   b</p>\n<!-- gone -->\n<pre>x\n   y</pre>\n</div>\n";
        assert_eq!(html(src), "<div> <p>a b</p> <pre>x\n   y</pre> </div>");
    }

    #[test]
    fn html_keeps_script_content() {
        let src = "<script>\n  if (a  <  b) {}\n</script>\n<p>x</p>";
        assert!(html(src).starts_with("<script>\n  if (a  <  b) {}\n</script>"));
    }

    #[test]
    fn html_survives_non_ascii_after_angle_bracket() {
        for src in [
            "a <“quoted” b",
            "<é>x</é>",
            "<日本>語</日本>",
            "1 < 2 “ok”",
            "<",
        ] {
            let _ = html(src); // must not panic
        }
        assert_eq!(html("<p>“hi”   there</p>"), "<p>“hi” there</p>");
    }

    #[test]
    fn css_minifies() {
        let src =
            "/* c */\nbody  {\n  color : red;\n  margin: 0 auto;\n}\n a > b { content: \"a  b\" }";
        assert_eq!(
            css(src),
            "body{color:red;margin:0 auto;}a>b{content:\"a  b\"}"
        );
    }
}
