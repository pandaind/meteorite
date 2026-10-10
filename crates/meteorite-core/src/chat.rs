//! Optional PopTalk integration: the bundled widget and the `chat_widget()` template function.
//!
//! Nothing here runs unless `chat.provider: poptalk` is set. The widget talks only to the
//! configured backend; no secret ever appears in the generated site.

use std::collections::HashMap;

use base64::Engine;
use sha2::{Digest, Sha256, Sha384};
use tera::{Tera, Value};

use crate::config::{ChatConfig, ChatLoad};
use crate::util::{html_escape, join_url};

/// Version of the widget in `assets/vendor/poptalk/`. Keep in sync when refreshing it.
pub const WIDGET_VERSION: &str = "1.1.0";

static WIDGET_JS: &str = include_str!("../assets/vendor/poptalk/poptalk.min.js");
static WIDGET_LICENSE: &str = include_str!("../assets/vendor/poptalk/LICENSE");

/// The widget as shipped in a site: where it goes and how to reference it.
pub struct BundledWidget {
    /// Output path relative to the site root, content-hashed for cache busting.
    pub path: String,
    pub bytes: Vec<u8>,
    /// Subresource Integrity value (`sha384-...`).
    pub integrity: String,
    pub license_path: &'static str,
    pub license: String,
}

pub fn bundled_widget() -> BundledWidget {
    let bytes = WIDGET_JS.as_bytes().to_vec();
    let digest = Sha256::digest(&bytes);
    let short: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
    let sri = base64::engine::general_purpose::STANDARD.encode(Sha384::digest(&bytes));
    BundledWidget {
        path: format!("assets/poptalk-{short}.js"),
        bytes,
        integrity: format!("sha384-{sri}"),
        license_path: "assets/poptalk-LICENSE.txt",
        license: format!(
            "PopTalk chat widget {WIDGET_VERSION}, https://github.com/pandaind/poptalk\n\n{WIDGET_LICENSE}"
        ),
    }
}

/// Make `chat_widget()` available to templates. It renders nothing when chat is disabled.
pub fn register(
    tera: &mut Tera,
    chat: &ChatConfig,
    base_url: &str,
    bundled: Option<&BundledWidget>,
) {
    let html = if chat.enabled() {
        snippet(chat, base_url, bundled)
    } else {
        String::new()
    };
    tera.register_function("chat_widget", move |_: &HashMap<String, Value>| {
        Ok(Value::String(html.clone()))
    });
}

/// `</script>` and HTML-comment openers must not appear inside an inline script.
fn json_for_script(v: &Value) -> String {
    v.to_string()
        .replace("</", "<\\/")
        .replace("<!--", "<\\!--")
}

fn snippet(chat: &ChatConfig, base_url: &str, bundled: Option<&BundledWidget>) -> String {
    let (src, integrity) = match (chat.widget.as_str(), bundled) {
        ("bundled", Some(b)) => (join_url(base_url, &b.path), Some(b.integrity.clone())),
        (url, _) => (url.to_string(), None),
    };
    let mut cfg = serde_object([
        ("src", Value::String(src)),
        (
            "api",
            Value::String(chat.api_url.trim_end_matches('/').to_string()),
        ),
        ("persona", Value::String(chat.persona.clone())),
        ("lang", Value::String(chat.lang.clone())),
        ("theme", Value::String(chat.theme.clone())),
        ("accent", Value::String(chat.accent.clone())),
        ("fonts", Value::String(chat.fonts.clone())),
        (
            "load",
            Value::String(
                match chat.load {
                    ChatLoad::Click => "click",
                    ChatLoad::Idle => "idle",
                    ChatLoad::Immediate => "immediate",
                }
                .to_string(),
            ),
        ),
    ]);
    if let (Some(i), Some(obj)) = (integrity, cfg.as_object_mut()) {
        obj.insert("integrity".to_string(), Value::String(i));
    }

    let label = html_escape(&chat.label);
    let launcher = if chat.load == ChatLoad::Click {
        format!(
            "<button type=\"button\" class=\"mt-chat\" data-mt-chat hidden aria-haspopup=\"dialog\" aria-label=\"{label}\">\
<svg width=\"20\" height=\"20\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.8\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"M21 12a8 8 0 0 1-11.6 7.1L4 20l1.1-4.6A8 8 0 1 1 21 12z\"/></svg>\
<span>{label}</span></button>"
        )
    } else {
        String::new()
    };
    format!(
        "{launcher}<style>{STYLE}</style><script>{}</script>",
        LOADER.replace("__CFG__", &json_for_script(&cfg))
    )
}

fn serde_object<const N: usize>(items: [(&str, Value); N]) -> Value {
    Value::Object(items.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// Themes may restyle `.mt-chat`; these defaults use the theme's variables when present.
const STYLE: &str = ".mt-chat{position:fixed;right:1.25rem;bottom:1.25rem;z-index:2147483000;display:inline-flex;align-items:center;gap:.5rem;padding:.7rem 1rem;border:0;border-radius:999px;font:600 .95rem/1 system-ui,sans-serif;cursor:pointer;background:var(--accent,#6c63ff);color:var(--on-accent,#fff);box-shadow:0 6px 24px rgba(0,0,0,.28)}.mt-chat[hidden]{display:none}.mt-chat:hover{filter:brightness(1.08)}.mt-chat:focus-visible{outline:2px solid currentColor;outline-offset:3px}.mt-chat[data-error] span::after{content:' (unavailable)'}";

/// Loads the widget, hands it the site's colour mode and accent, and keeps them in sync when the
/// visitor flips light/dark or changes skin. Uses the widget's `window.PopTalk` API.
const LOADER: &str = r#"(function(){var C=__CFG__,root=document.documentElement,started=false,btn=document.querySelector('[data-mt-chat]');
function mode(){if(C.theme!=='auto')return C.theme;var t=root.getAttribute('data-theme');if(t==='light'||t==='dark')return t;return window.matchMedia&&matchMedia('(prefers-color-scheme: light)').matches?'light':'dark'}
function accent(){if(C.accent!=='auto')return C.accent;try{return getComputedStyle(root).getPropertyValue('--accent').trim()}catch(e){return ''}}
function sync(){var p=window.PopTalk;if(!p)return;p.setTheme(mode());var a=accent();if(a)p.setAccent(a)}
function load(open){if(started){if(open&&window.PopTalk)window.PopTalk.open();return}started=true;
var s=document.createElement('script');s.src=C.src;s.async=true;if(C.integrity){s.integrity=C.integrity;s.crossOrigin='anonymous'}
s.setAttribute('data-api-url',C.api);if(C.persona)s.setAttribute('data-persona',C.persona);if(C.lang)s.setAttribute('data-lang',C.lang);
s.setAttribute('data-fonts',C.fonts);s.setAttribute('data-theme',mode());var a=accent();if(a)s.setAttribute('data-accent',a);
s.onload=function(){if(btn)btn.hidden=true;sync();if(open&&window.PopTalk)window.PopTalk.open()};
s.onerror=function(){started=false;if(btn){btn.hidden=false;btn.setAttribute('data-error','1')}};
document.body.appendChild(s)}
if(window.MutationObserver)new MutationObserver(function(){requestAnimationFrame(sync)}).observe(root,{attributes:true,attributeFilter:['data-theme','data-skin']});
if(C.load==='click'){if(btn){btn.hidden=false;btn.addEventListener('click',function(){load(true)})}}
else if(C.load==='idle'){var go=function(){load(false)};if('requestIdleCallback' in window)requestIdleCallback(go,{timeout:4000});else setTimeout(go,2000)}
else load(false)})();"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled() -> ChatConfig {
        ChatConfig {
            provider: "poptalk".into(),
            api_url: "https://chat.example.com/".into(),
            persona: "docs".into(),
            ..ChatConfig::default()
        }
    }

    #[test]
    fn bundled_widget_has_stable_hashed_name_and_sri() {
        let w = bundled_widget();
        assert!(w.path.starts_with("assets/poptalk-") && w.path.ends_with(".js"));
        assert!(w.integrity.starts_with("sha384-"));
        assert!(w.license.contains("MIT License"));
        assert_eq!(w.path, bundled_widget().path, "name must be deterministic");
    }

    #[test]
    fn snippet_wires_backend_persona_and_integrity() {
        let w = bundled_widget();
        let html = snippet(&enabled(), "https://site.test/", Some(&w));
        assert!(
            html.contains("\"api\":\"https://chat.example.com\""),
            "trailing slash trimmed"
        );
        assert!(html.contains("\"persona\":\"docs\""));
        assert!(html.contains(&format!("https://site.test/{}", w.path)));
        assert!(html.contains(&w.integrity));
        assert!(
            html.contains("data-mt-chat"),
            "click mode renders a launcher"
        );
    }

    #[test]
    fn idle_and_immediate_have_no_launcher_and_custom_urls_have_no_sri() {
        let mut c = enabled();
        c.load = ChatLoad::Immediate;
        c.widget = "https://cdn.example.com/poptalk.js".into();
        let html = snippet(&c, "/", None);
        assert!(!html.contains("data-mt-chat hidden"));
        assert!(
            html.contains("https://cdn.example.com/poptalk.js") && !html.contains("integrity\":")
        );
    }

    #[test]
    fn script_content_cannot_close_the_script_tag() {
        let mut c = enabled();
        c.label = "</script><script>alert(1)</script>".into();
        c.persona = String::new();
        let html = snippet(&c, "/", None);
        // The label only lands in the (escaped) button text, never in the script.
        assert!(!html.contains("<script>alert"));
        let script = &html[html.find("<script>").unwrap()..];
        assert_eq!(script.matches("</script>").count(), 1);
    }
}
