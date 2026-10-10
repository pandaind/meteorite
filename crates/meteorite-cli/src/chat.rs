//! `meteorite chat check` and `meteorite chat sync`.

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use meteorite_core::config::Config;
use meteorite_core::Error;

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("meteorite/", env!("CARGO_PKG_VERSION")))
        .build()
}

/// `https://example.com/docs/` -> `https://example.com`
fn origin_of(url: &str) -> String {
    match url.split_once("://") {
        Some((scheme, rest)) => {
            let host = rest.split('/').next().unwrap_or(rest);
            format!("{scheme}://{host}")
        }
        None => url.trim_end_matches('/').to_string(),
    }
}

/// Is `origin` allowed by an `Access-Control-Allow-Origin` header value?
fn cors_allows(header: Option<&str>, origin: &str) -> bool {
    matches!(header, Some("*")) || header == Some(origin)
}

pub fn check(root: &Path) -> Result<ExitCode, Error> {
    let cfg = Config::load(root)?;
    let chat = &cfg.chat;
    if !chat.enabled() {
        println!("Chat is not enabled. Add to config.yml:\n\nchat:\n  provider: poptalk\n  api_url: https://chat.example.com");
        return Ok(ExitCode::from(1));
    }
    let api = chat.api_url.trim_end_matches('/');
    let mut url = format!("{api}/api/config");
    if !chat.persona.is_empty() {
        url.push_str(&format!("?persona={}", chat.persona));
    }
    let site_origin = origin_of(&cfg.base_url);
    println!("Checking {url}");

    let resp = match agent().get(&url).set("Origin", &site_origin).call() {
        Ok(r) => r,
        Err(ureq::Error::Status(code, _)) => {
            println!(
                "  x The backend answered HTTP {code}. Is `api_url` the PopTalk backend root?"
            );
            return Ok(ExitCode::from(1));
        }
        Err(e) => {
            println!("  x Could not reach the backend: {e}");
            return Ok(ExitCode::from(1));
        }
    };

    let allow = resp
        .header("access-control-allow-origin")
        .map(str::to_string);
    let body: serde_json::Value = resp
        .into_json()
        .map_err(|e| Error::Msg(format!("the backend did not return JSON: {e}")))?;
    let field = |k: &str| body.get(k).and_then(|v| v.as_str()).unwrap_or("-");
    println!("  ok Backend is up");
    println!("     title:  {}", field("chatTitle"));
    println!("     owner:  {}", field("ownerName"));
    println!("     mode:   {}", field("chatMode"));

    let mut problems = 0;
    if cors_allows(allow.as_deref(), &site_origin) {
        println!("  ok CORS allows {site_origin}");
    } else {
        problems += 1;
        println!(
            "  x CORS: the backend sent `Access-Control-Allow-Origin: {}`, which does not allow {site_origin}.\n     Set CORS_ALLOWED_ORIGIN={site_origin} on the backend (and your local origin while developing).",
            allow.as_deref().unwrap_or("(none)")
        );
    }
    if site_origin.starts_with("https://") && api.starts_with("http://") {
        problems += 1;
        println!(
            "  x The site is https but api_url is http: browsers block this as mixed content."
        );
    }
    if cfg.base_url.starts_with('/') {
        println!("  ! base_url is not absolute, so the origin above is a guess; set it to your real URL.");
    }
    Ok(if problems == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

pub fn sync(root: &Path) -> Result<ExitCode, Error> {
    let cfg = Config::load(root)?;
    let k = &cfg.chat.knowledge;
    if k.rag_url.is_empty() {
        return Err(Error::Msg(
            "config.yml: set chat.knowledge.rag_url to your poptalk-rag base URL to use `chat sync`".into(),
        ));
    }
    let key = std::env::var(&k.key_env).map_err(|_| {
        Error::Msg(format!(
            "the environment variable {} is not set. It must hold the persona's poptalk-rag API key; \
             it is read from the environment so it never appears in config.yml or the built site",
            k.key_env
        ))
    })?;
    let url = format!("{}/admin/reindex", k.rag_url.trim_end_matches('/'));
    match agent()
        .post(&url)
        .set("Authorization", &format!("Bearer {key}"))
        .send_string("")
    {
        Ok(r) => {
            println!("Re-index requested at {url} (HTTP {})", r.status());
            Ok(ExitCode::SUCCESS)
        }
        Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _)) => {
            Err(Error::Msg(format!(
                "poptalk-rag rejected the key in {} (check PERSONA_API_KEYS on the server)",
                k.key_env
            )))
        }
        Err(ureq::Error::Status(code, _)) => Err(Error::Msg(format!(
            "poptalk-rag answered HTTP {code} for {url}"
        ))),
        Err(e) => Err(Error::Msg(format!("could not reach {url}: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins() {
        assert_eq!(
            origin_of("https://pandaind.github.io/meteorite/"),
            "https://pandaind.github.io"
        );
        assert_eq!(origin_of("http://127.0.0.1:1111/"), "http://127.0.0.1:1111");
        assert_eq!(origin_of("https://example.com"), "https://example.com");
    }

    #[test]
    fn cors_matching_is_exact() {
        assert!(cors_allows(Some("*"), "https://a.test"));
        assert!(cors_allows(Some("https://a.test"), "https://a.test"));
        assert!(!cors_allows(Some("https://b.test"), "https://a.test"));
        assert!(!cors_allows(None, "https://a.test"));
    }
}
