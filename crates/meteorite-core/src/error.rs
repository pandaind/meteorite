use std::path::{Path, PathBuf};

use miette::{Diagnostic, NamedSource, SourceSpan};

#[derive(Debug, thiserror::Error, Diagnostic)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid YAML: {message}")]
    #[diagnostic(code(meteorite::yaml))]
    Yaml {
        message: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("here")]
        span: SourceSpan,
    },

    #[error("front matter opened with `---` but never closed")]
    #[diagnostic(
        code(meteorite::front_matter),
        help("add a closing `---` line after the YAML")
    )]
    UnclosedFrontMatter {
        #[source_code]
        src: NamedSource<String>,
        #[label("front matter starts here")]
        span: SourceSpan,
    },

    #[error("theme `{0}` not found (looked in themes/{0})")]
    #[diagnostic(
        code(meteorite::theme_not_found),
        help("install it with `meteorite theme add <name|path|git-url>` or run `meteorite theme list`")
    )]
    ThemeNotFound(String),

    #[error("theme `{theme}` is invalid: {reason}")]
    #[diagnostic(code(meteorite::theme_invalid))]
    ThemeInvalid { theme: String, reason: String },

    #[error("theme `{theme}` requires params that are not set: {params}")]
    #[diagnostic(
        code(meteorite::theme_params),
        help("set them under `params:` in config.yml")
    )]
    MissingThemeParams { theme: String, params: String },

    #[error("template error: {0}")]
    #[diagnostic(code(meteorite::template))]
    Template(String),

    #[error("template syntax error: {message}")]
    #[diagnostic(code(meteorite::template_syntax))]
    TemplateSyntax {
        message: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("here")]
        span: SourceSpan,
    },

    #[error("{0} already exists")]
    AlreadyExists(PathBuf),

    #[error("unsafe or invalid path: {0}")]
    #[diagnostic(code(meteorite::path))]
    InvalidPath(String),

    #[error("two pages produce the URL {url}: {a} and {b}")]
    #[diagnostic(code(meteorite::url_collision))]
    UrlCollision { url: String, a: PathBuf, b: PathBuf },

    #[error("{file}: {message}")]
    #[diagnostic(code(meteorite::content))]
    Content { file: PathBuf, message: String },

    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn io_err(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> Error {
    let path = path.into();
    move |source| Error::Io { path, source }
}

impl Error {
    /// Build a YAML error that points at the offending spot in `full_text`.
    /// `offset` is where the YAML fragment begins inside `full_text`.
    pub fn yaml(path: &Path, full_text: &str, offset: usize, e: &serde_norway::Error) -> Error {
        let idx = e.location().map(|l| l.index()).unwrap_or(0);
        let at = (offset + idx).min(full_text.len().saturating_sub(1));
        Error::Yaml {
            message: e.to_string(),
            src: NamedSource::new(path.display().to_string(), full_text.to_string()),
            span: (at, 1).into(),
        }
    }

    pub(crate) fn content(file: impl Into<PathBuf>, message: impl Into<String>) -> Error {
        Error::Content {
            file: file.into(),
            message: message.into(),
        }
    }
}

/// Flatten a Tera error and its causes into one message. Syntax errors that carry a
/// location become a diagnostic that points into the template.
pub(crate) fn tera_err(e: tera::Error) -> Error {
    use std::error::Error as _;
    let mut msg = e.to_string();
    let mut cur = e.source();
    while let Some(s) = cur {
        msg.push_str(": ");
        msg.push_str(&s.to_string());
        cur = s.source();
    }
    syntax_diagnostic(&msg).unwrap_or(Error::Template(msg))
}

/// Tera reports parse errors as `Failed to parse "<path>": --> <line>:<col> ... = <expected>`.
fn syntax_diagnostic(msg: &str) -> Option<Error> {
    let start = msg.find("Failed to parse \"")? + "Failed to parse \"".len();
    let path = &msg[start..start + msg[start..].find('"')?];
    let arrow = msg.find("--> ")? + 4;
    let loc = &msg[arrow..];
    let (line, rest) = loc.split_once(':')?;
    let line: usize = line.trim().parse().ok()?;
    let col: usize = rest
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()?;
    let detail = msg
        .lines()
        .rev()
        .find_map(|l| l.trim().strip_prefix("= "))
        .unwrap_or("could not parse this template");

    let text = std::fs::read_to_string(path).ok()?;
    let line_start: usize = text
        .split_inclusive('\n')
        .take(line.checked_sub(1)?)
        .map(str::len)
        .sum();
    let line_text = text[line_start..].split('\n').next().unwrap_or("");
    let col_bytes = line_text
        .char_indices()
        .nth(col.saturating_sub(1))
        .map(|(i, _)| i)
        .unwrap_or(0);
    Some(Error::TemplateSyntax {
        message: detail.to_string(),
        src: NamedSource::new(path, text),
        span: (line_start + col_bytes, 1).into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tera_syntax_location() {
        let dir = std::env::temp_dir().join(format!("meteorite-err-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("bad.html");
        std::fs::write(&file, "<p>ok</p>\n{% if x %}\n<b>{{ a | }}</b>\n").unwrap();
        let msg = format!(
            "Failed to parse \"{}\": --> 3:11\n  |\n3 | <b>{{{{ a | }}}}</b>\n  |           ^---\n  |\n  = expected a filter name",
            file.display()
        );
        match syntax_diagnostic(&msg) {
            Some(Error::TemplateSyntax { message, span, .. }) => {
                assert_eq!(message, "expected a filter name");
                let text = std::fs::read_to_string(&file).unwrap();
                assert_eq!(&text[span.offset()..span.offset() + 1], "}");
            }
            other => panic!("unexpected: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
