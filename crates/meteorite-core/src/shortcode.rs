//! Shortcodes: `{{< name key="value" >}}body{{< /name >}}` or `{{< name key=1 />}}`.
//!
//! Expansion happens before Markdown parsing, and never touches fenced code
//! blocks or inline code spans, so documentation can show shortcode syntax.

use std::collections::BTreeMap;

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

pub type Args = BTreeMap<String, Arg>;

/// Callback that turns `(name, args, inner body)` into HTML.
pub type Renderer<'a> = dyn FnMut(&str, &Args, Option<&str>) -> Result<String> + 'a;

pub fn expand(body: &str, render: &mut Renderer<'_>) -> Result<String> {
    if !body.contains("{{<") {
        return Ok(body.to_string());
    }
    let mut out = String::with_capacity(body.len());
    let mut segment = String::new();
    let mut fence: Option<(char, usize)> = None;

    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let marker = fence_marker(trimmed);
        match (fence, marker) {
            (None, Some(m)) => {
                out.push_str(&expand_text(&segment, render)?);
                segment.clear();
                fence = Some(m);
                out.push_str(line);
            }
            (Some((c, n)), Some((c2, n2)))
                if c == c2 && n2 >= n && trimmed[n2..].trim().is_empty() =>
            {
                fence = None;
                out.push_str(line);
            }
            (Some(_), _) => out.push_str(line),
            (None, None) => segment.push_str(line),
        }
    }
    out.push_str(&expand_text(&segment, render)?);
    Ok(out)
}

fn fence_marker(trimmed: &str) -> Option<(char, usize)> {
    let c = trimmed.chars().next()?;
    if c != '`' && c != '~' {
        return None;
    }
    let n = trimmed.chars().take_while(|&x| x == c).count();
    (n >= 3).then_some((c, n))
}

fn expand_text(text: &str, render: &mut Renderer<'_>) -> Result<String> {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < text.len() {
        if bytes[i] == b'`' {
            // Copy an inline code span verbatim.
            let run = text[i..].bytes().take_while(|&b| b == b'`').count();
            let ticks = &text[i..i + run];
            let after = i + run;
            match text[after..].find(ticks) {
                Some(end) => {
                    out.push_str(&text[i..after + end + run]);
                    i = after + end + run;
                }
                None => {
                    out.push_str(&text[i..after]);
                    i = after;
                }
            }
        } else if text[i..].starts_with("{{<") {
            let close = text[i..]
                .find(">}}")
                .ok_or_else(|| Error::Msg("unterminated shortcode: missing `>}}`".into()))?;
            let inner = text[i + 3..i + close].trim();
            let next = i + close + 3;
            let (self_closing, inner) = match inner.strip_suffix('/') {
                Some(s) => (true, s.trim_end()),
                None => (false, inner),
            };
            if inner.starts_with('/') {
                return Err(Error::Msg(format!(
                    "closing shortcode `{{{{< {inner} >}}}}` has no opening tag"
                )));
            }
            let (name, args) = parse_call(inner)?;
            let (inner_body, resume) = if self_closing {
                (None, next)
            } else {
                let closing = format!("{{{{< /{name} >}}}}");
                let end = text[next..].find(&closing).ok_or_else(|| {
                    Error::Msg(format!(
                        "shortcode `{name}` is not closed; add `{closing}` or write it as `{{{{< {name} />}}}}`"
                    ))
                })?;
                (Some(&text[next..next + end]), next + end + closing.len())
            };
            let html = render(&name, &args, inner_body)?;
            let at_line_start = out.is_empty() || out.ends_with('\n');
            let at_line_end = text[resume..].starts_with('\n') || resume >= text.len();
            if at_line_start && at_line_end {
                out.push('\n');
                out.push_str(html.trim());
                out.push('\n');
            } else {
                out.push_str(&html);
            }
            i = resume;
        } else {
            let ch = text[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    Ok(out)
}

fn parse_call(s: &str) -> Result<(String, Args)> {
    let mut chars = s.char_indices().peekable();
    let name_end = loop {
        match chars.peek() {
            Some(&(i, c)) if c.is_whitespace() => break i,
            Some(_) => {
                chars.next();
            }
            None => break s.len(),
        }
    };
    let name = &s[..name_end];
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Error::Msg(format!("invalid shortcode name `{name}`")));
    }
    let mut args = Args::new();
    let mut rest = s[name_end..].trim_start();
    while !rest.is_empty() {
        let eq = rest.find('=').ok_or_else(|| {
            Error::Msg(format!(
                "shortcode `{name}`: expected key=value near `{rest}`"
            ))
        })?;
        let key = rest[..eq].trim();
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(Error::Msg(format!(
                "shortcode `{name}`: invalid argument name `{key}`"
            )));
        }
        rest = rest[eq + 1..].trim_start();
        let (value, tail) = if let Some(q) = rest.strip_prefix('"') {
            let end = q.find('"').ok_or_else(|| {
                Error::Msg(format!(
                    "shortcode `{name}`: unterminated string for `{key}`"
                ))
            })?;
            (Arg::Str(q[..end].to_string()), &q[end + 1..])
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            (bare(&rest[..end]), &rest[end..])
        };
        args.insert(key.to_string(), value);
        rest = tail.trim_start();
    }
    Ok((name.to_string(), args))
}

fn bare(s: &str) -> Arg {
    match s {
        "true" => Arg::Bool(true),
        "false" => Arg::Bool(false),
        _ => s
            .parse::<i64>()
            .map(Arg::Int)
            .or_else(|_| s.parse::<f64>().map(Arg::Float))
            .unwrap_or_else(|_| Arg::Str(s.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Result<String> {
        expand(src, &mut |name, args, body| {
            let mut s = format!("[{name}");
            for (k, v) in args {
                s.push_str(&format!(" {k}={v:?}"));
            }
            if let Some(b) = body {
                s.push_str(&format!(" body={b:?}"));
            }
            s.push(']');
            Ok(s)
        })
    }

    #[test]
    fn self_closing_and_paired() {
        assert_eq!(
            run("a {{< yt id=\"x\" n=3 />}} b").unwrap(),
            "a [yt id=Str(\"x\") n=Int(3)] b"
        );
        assert_eq!(
            run("{{< note >}}hi{{< /note >}}").unwrap().trim(),
            "[note body=\"hi\"]"
        );
    }

    #[test]
    fn code_is_untouched() {
        let src = "```\n{{< yt />}}\n```\nand `{{< yt />}}` here";
        assert_eq!(run(src).unwrap(), src);
    }

    #[test]
    fn errors() {
        assert!(run("{{< note >}}never closed").is_err());
        assert!(run("{{< /note >}}").is_err());
        assert!(run("{{< x a=\"open />}}").is_err());
    }
}
