//! `meteorite serve`: static file server with file watching, live reload, and a dev-only
//! toolbar for previewing any installed theme without editing `config.yml`.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use meteorite_core::build::{BuildOptions, BuildReport, Builder};
use meteorite_core::config::Config;
use meteorite_core::{theme, Error};
use notify::{RecursiveMode, Watcher};
use tokio::sync::broadcast;

const WS_PATH: &str = "/__meteorite/ws";
const THEME_PATH: &str = "/__meteorite/theme";
const COOKIE: &str = "meteorite-dev-theme";

/// Everything the HTTP handlers and the rebuild thread share.
struct Dev {
    root: PathBuf,
    /// Options for the first theme; alternates add `theme` and `out_dir` to a copy.
    opts: BuildOptions,
    /// Output directory of the first theme (the normal `public/`).
    public: PathBuf,
    /// Name of the theme the server started with.
    primary: String,
    builders: Mutex<HashMap<String, Builder>>,
    /// Themes that have been built at least once, and so can be served.
    built: RwLock<HashSet<String>>,
}

impl Dev {
    /// Alternates live under `public/_preview/<theme>`; the URL space stays the same, the
    /// theme is chosen per request from a cookie.
    fn out_dir(&self, theme: &str) -> PathBuf {
        if theme == self.primary {
            self.public.clone()
        } else {
            self.public.join("_preview").join(theme)
        }
    }

    fn opts_for(&self, theme: &str) -> BuildOptions {
        if theme == self.primary {
            self.opts.clone()
        } else {
            BuildOptions {
                theme: Some(theme.to_string()),
                out_dir: Some(self.out_dir(theme)),
                ..self.opts.clone()
            }
        }
    }

    fn is_built(&self, theme: &str) -> bool {
        self.built
            .read()
            .map(|b| b.contains(theme))
            .unwrap_or(false)
    }

    /// Build (or rebuild) one theme. Used for the first build, on demand, and on file changes.
    fn build_theme(&self, theme: &str) -> Result<BuildReport, Error> {
        let opts = self.opts_for(theme);
        let mut builders = self.builders.lock().unwrap();
        let builder = builders
            .entry(theme.to_string())
            .or_insert_with(|| Builder::new(&self.root));
        let report = builder.build(&opts)?;
        self.built.write().unwrap().insert(theme.to_string());
        Ok(report)
    }

    /// Rebuild every theme that has been viewed so far.
    fn rebuild_all(&self) -> Vec<(String, Result<BuildReport, Error>)> {
        let names: Vec<String> = self.built.read().unwrap().iter().cloned().collect();
        let mut out = Vec::new();
        // Primary first so errors in the main theme are reported first.
        let mut ordered = vec![self.primary.clone()];
        ordered.extend(names.into_iter().filter(|n| *n != self.primary));
        for name in ordered {
            let r = self.build_theme(&name);
            out.push((name, r));
        }
        out
    }

    /// Valid installed themes, for the toolbar.
    fn installed(&self) -> Vec<String> {
        theme::list_installed(&self.root)
            .map(|v| {
                v.into_iter()
                    .filter(|t| t.manifest.is_some())
                    .map(|t| t.dir_name)
                    .collect()
            })
            .unwrap_or_default()
    }
}

struct AppState {
    dev: Arc<Dev>,
    reload: broadcast::Sender<()>,
}

/// The host to put in links. `0.0.0.0` (all interfaces, what you bind inside a container) is not an
/// address a browser can be sent to, so advertise the loopback address you would actually type.
fn advertised_host(bind: &str) -> String {
    match bind {
        "0.0.0.0" => "127.0.0.1".to_string(),
        "::" | "[::]" => "[::1]".to_string(),
        other if other.contains(':') && !other.starts_with('[') => format!("[{other}]"),
        other => other.to_string(),
    }
}

/// Where `serve` writes the preview: a folder in the system temp directory that is unique to
/// the site root, so it never collides with `public/` or with another site's preview.
fn preview_dir(root: &Path) -> PathBuf {
    let abs = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    // FNV-1a: stable across runs and platforms, unlike `DefaultHasher`.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in abs.to_string_lossy().as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    std::env::temp_dir().join(format!("meteorite-serve-{hash:016x}"))
}

pub fn run(
    root: &Path,
    mut opts: BuildOptions,
    bind: &str,
    port: u16,
    open: bool,
) -> Result<ExitCode, Error> {
    let origin = format!("http://{}:{port}", advertised_host(bind));
    opts.base_url = Some(format!("{origin}/"));
    // The preview has its own output folder, so a production `meteorite build` run in another
    // terminal cannot overwrite it with pages that point at the live site.
    if opts.out_dir.is_none() {
        opts.out_dir = Some(preview_dir(root));
    }
    // Previewing must not write into another project's knowledge folder.
    opts.skip_knowledge_export = true;

    let primary = match &opts.theme {
        Some(t) => t.clone(),
        None => Config::load(root)?.theme,
    };
    // Build the first theme up front so errors show immediately.
    let probe = Dev {
        root: root.to_path_buf(),
        opts: opts.clone(),
        public: PathBuf::new(),
        primary: primary.clone(),
        builders: Mutex::new(HashMap::new()),
        built: RwLock::new(HashSet::new()),
    };
    let mut first = Builder::new(root);
    let report = first.build(&opts)?;
    println!(
        "Built {} page(s) in {} ms (theme: {primary})",
        report.pages,
        report.elapsed.as_millis()
    );
    for w in &report.warnings {
        eprintln!("warning: {w}");
    }
    let dev = Arc::new(Dev {
        public: report.output.clone(),
        builders: Mutex::new(HashMap::from([(primary.clone(), first)])),
        built: RwLock::new(HashSet::from([primary.clone()])),
        ..probe
    });

    let (reload_tx, _) = broadcast::channel::<()>(16);
    let (event_tx, event_rx) = mpsc::channel::<()>();

    // Watch the source directories that influence the output.
    let root_abs = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let out_abs = dev
        .public
        .canonicalize()
        .unwrap_or_else(|_| dev.public.clone());
    let root_for_events = root_abs.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            // Files directly in the site root only matter if it's config.yml.
            let relevant = ev.paths.iter().any(|p| {
                if p.starts_with(&out_abs) {
                    return false;
                }
                if p.parent() == Some(root_for_events.as_path()) {
                    return p.file_name().is_some_and(|n| n == "config.yml");
                }
                true
            });
            if relevant && !matches!(ev.kind, notify::EventKind::Access(_)) {
                let _ = event_tx.send(());
            }
        }
    })
    .map_err(|e| Error::Msg(format!("could not start file watcher: {e}")))?;
    for dir in ["content", "static", "templates", "themes", "data"] {
        let p = root_abs.join(dir);
        if p.is_dir() {
            let _ = watcher.watch(&p, RecursiveMode::Recursive);
        }
    }
    let _ = watcher.watch(&root_abs, RecursiveMode::NonRecursive); // config.yml

    // Rebuild thread: debounce bursts of events, rebuild, then tell browsers to reload.
    let tx = reload_tx.clone();
    let rebuild_dev = dev.clone();
    std::thread::spawn(move || {
        while event_rx.recv().is_ok() {
            while event_rx.recv_timeout(Duration::from_millis(120)).is_ok() {}
            let mut any_ok = false;
            for (name, result) in rebuild_dev.rebuild_all() {
                match result {
                    Ok(r) => {
                        any_ok = true;
                        println!(
                            "Rebuilt {name} in {} ms ({} written, {} unchanged)",
                            r.elapsed.as_millis(),
                            r.files_written,
                            r.files_unchanged
                        );
                        for w in &r.warnings {
                            eprintln!("warning: {w}");
                        }
                    }
                    Err(e) => eprintln!("{:?}", miette::Report::new(e)),
                }
            }
            if any_ok {
                let _ = tx.send(());
            }
        }
    });

    let state = Arc::new(AppState {
        dev,
        reload: reload_tx,
    });
    let addr = format!("{bind}:{port}");
    let rt = tokio::runtime::Runtime::new().map_err(|e| Error::Msg(e.to_string()))?;
    rt.block_on(async move {
        let app = Router::new()
            .route(WS_PATH, get(ws_handler))
            .route(THEME_PATH, get(theme_handler))
            .fallback(get(file_handler))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .map_err(|e| Error::Msg(format!("could not listen on {addr}: {e}")))?;
        println!("Serving {origin}/  (Ctrl-C to stop)");
        if open {
            open_browser(&origin);
        }
        axum::serve(listener, app)
            .await
            .map_err(|e| Error::Msg(e.to_string()))
    })?;
    drop(watcher);
    Ok(ExitCode::SUCCESS)
}

fn open_browser(url: &str) {
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(cmd).arg(url).spawn();
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> Response {
    ws.on_upgrade(move |socket| reload_loop(socket, state))
}

async fn reload_loop(mut socket: WebSocket, state: Arc<AppState>) {
    let mut rx = state.reload.subscribe();
    loop {
        tokio::select! {
            r = rx.recv() => {
                if r.is_err() && matches!(r, Err(broadcast::error::RecvError::Closed)) { break; }
                if socket.send(Message::Text("reload".into())).await.is_err() { break; }
            }
            msg = socket.recv() => {
                if msg.is_none() { break; }
            }
        }
    }
}

/// Map a request path to a file below `out_dir`, refusing anything that could escape it.
fn resolve(out_dir: &Path, request_path: &str) -> Option<PathBuf> {
    let decoded = percent_decode(request_path);
    let mut path = out_dir.to_path_buf();
    for comp in Path::new(decoded.trim_start_matches('/')).components() {
        match comp {
            Component::Normal(c) => path.push(c),
            Component::CurDir => {}
            _ => return None,
        }
    }
    if decoded.contains('\\') || decoded.contains('\0') {
        return None;
    }
    if path.is_dir() {
        path.push("index.html");
    }
    Some(path)
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "xml" => "application/xml",
        "txt" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

const RELOAD_SCRIPT: &str = "<script>(function(){var s=new WebSocket((location.protocol==='https:'?'wss://':'ws://')+location.host+'/__meteorite/ws');s.onmessage=function(){location.reload()};s.onclose=function(){setTimeout(function(){location.reload()},1000)}})();</script>";

/// A small floating theme picker, isolated in a shadow root so it can't be styled by (or
/// break) the site. Only served by `meteorite serve`; never part of a build.
fn toolbar_script(themes: &[String], active: &str) -> String {
    // Names are restricted to [A-Za-z0-9_-] by `theme::check_name`, so plain quoting is safe.
    let list = themes
        .iter()
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "<script>(function(){{var T=[{list}],A=\"{active}\";if(T.length<2)return;\
try{{if(sessionStorage.getItem('meteorite-dev-bar')==='hidden')return}}catch(e){{}}\
var h=document.createElement('div');h.setAttribute('data-meteorite-dev','');\
h.style.cssText='position:fixed;left:14px;bottom:14px;z-index:2147483647;all:initial;position:fixed;left:14px;bottom:14px;z-index:2147483647';\
var r=h.attachShadow({{mode:'open'}});\
r.innerHTML='<style>.b{{font:12px/1 system-ui,sans-serif;display:flex;align-items:center;gap:8px;padding:8px 10px;border-radius:10px;background:#12141f;color:#e8e9f3;border:1px solid #2c2f45;box-shadow:0 8px 30px rgba(0,0,0,.35)}}select{{font:inherit;color:inherit;background:#1b1e2e;border:1px solid #3a3e5a;border-radius:6px;padding:4px 6px}}button{{font:inherit;color:#9a9eb8;background:none;border:0;cursor:pointer;padding:0 2px}}span{{opacity:.7}}</style><div class=\"b\"><span>Theme</span><select aria-label=\"Preview theme\"></select><button aria-label=\"Hide theme picker\" title=\"Hide\">\\u00d7</button></div>';\
var s=r.querySelector('select');T.forEach(function(t){{var o=document.createElement('option');o.value=t;o.textContent=t;if(t===A)o.selected=true;s.appendChild(o)}});\
s.addEventListener('change',function(){{location.href='{THEME_PATH}?name='+encodeURIComponent(s.value)+'&next='+encodeURIComponent(location.pathname+location.search)}});\
r.querySelector('button').addEventListener('click',function(){{try{{sessionStorage.setItem('meteorite-dev-bar','hidden')}}catch(e){{}}h.remove()}});\
document.body.appendChild(h)}})();</script>"
    )
}

fn with_dev_scripts(html: Vec<u8>, themes: &[String], active: &str) -> Vec<u8> {
    let text = String::from_utf8_lossy(&html).into_owned();
    let inject = format!("{RELOAD_SCRIPT}{}", toolbar_script(themes, active));
    match text.rfind("</body>") {
        Some(at) => format!("{}{}{}", &text[..at], inject, &text[at..]).into_bytes(),
        None => format!("{text}{inject}").into_bytes(),
    }
}

/// The theme chosen by the dev cookie, if it names a theme that has been built.
fn cookie_theme(headers: &HeaderMap, dev: &Dev) -> String {
    let raw = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    for part in raw.split(';') {
        if let Some((k, v)) = part.trim().split_once('=') {
            if k == COOKIE && theme::check_name(v).is_ok() && dev.is_built(v) {
                return v.to_string();
            }
        }
    }
    dev.primary.clone()
}

async fn file_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let dev = &state.dev;
    let active = cookie_theme(&headers, dev);
    let out_dir = dev.out_dir(&active);
    let Some(path) = resolve(&out_dir, uri.path()) else {
        return (StatusCode::BAD_REQUEST, "bad path").into_response();
    };
    let (path, status) = if path.is_file() {
        (path, StatusCode::OK)
    } else {
        let not_found = out_dir.join("404.html");
        if not_found.is_file() {
            (not_found, StatusCode::NOT_FOUND)
        } else {
            return (StatusCode::NOT_FOUND, "404 not found").into_response();
        }
    };
    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let ct = content_type(&path);
            let body = if ct.starts_with("text/html") {
                with_dev_scripts(bytes, &dev.installed(), &active)
            } else {
                bytes
            };
            (
                status,
                [
                    (header::CONTENT_TYPE, ct),
                    (header::CACHE_CONTROL, "no-store"),
                ],
                body,
            )
                .into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "404 not found").into_response(),
    }
}

/// `GET /__meteorite/theme?name=<theme>&next=<path>`: build the theme if needed, remember
/// the choice in a cookie, and send the browser back to where it was.
async fn theme_handler(State(state): State<Arc<AppState>>, uri: Uri) -> Response {
    let query = uri.query().unwrap_or("");
    let param = |key: &str| -> Option<String> {
        query.split('&').find_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            (k == key).then(|| percent_decode(&v.replace('+', " ")))
        })
    };
    let Some(name) = param("name") else {
        return (StatusCode::BAD_REQUEST, "missing `name`").into_response();
    };
    if theme::check_name(&name).is_err() || !state.dev.installed().contains(&name) {
        return (StatusCode::BAD_REQUEST, format!("unknown theme `{name}`")).into_response();
    }
    let next = safe_next(param("next").as_deref());

    if !state.dev.is_built(&name) {
        let dev = state.dev.clone();
        let theme_name = name.clone();
        let result = tokio::task::spawn_blocking(move || dev.build_theme(&theme_name)).await;
        match result {
            Ok(Ok(report)) => {
                println!("Built theme {name} in {} ms", report.elapsed.as_millis());
            }
            Ok(Err(e)) => {
                let summary = e.to_string();
                eprintln!("{:?}", miette::Report::new(e));
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Could not build theme `{name}`:\n\n{summary}\n\nSee the terminal for details."),
                )
                    .into_response();
            }
            Err(e) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
        }
    }
    (
        StatusCode::FOUND,
        [
            (header::LOCATION, next),
            (
                header::SET_COOKIE,
                format!("{COOKIE}={name}; Path=/; SameSite=Lax"),
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
    )
        .into_response()
}

/// Only same-site absolute paths are accepted as redirect targets.
fn safe_next(next: Option<&str>) -> String {
    match next {
        Some(n)
            if n.starts_with('/')
                && !n.starts_with("//")
                && !n.contains('\\')
                && !n.chars().any(|c| c.is_control()) =>
        {
            n.to_string()
        }
        _ => "/".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_blocks_traversal() {
        let out = Path::new("/srv/public");
        assert!(resolve(out, "/../etc/passwd").is_none());
        assert!(resolve(out, "/a/%2e%2e/%2e%2e/etc/passwd").is_none());
        assert!(resolve(out, "/a\\..\\b").is_none());
        assert_eq!(
            resolve(out, "/a/b.css"),
            Some(PathBuf::from("/srv/public/a/b.css"))
        );
        // Malformed escapes and non-ASCII must neither panic nor escape the directory.
        assert!(resolve(out, "/%é/x").is_some());
        assert!(resolve(out, "/100%").is_some());
        assert!(resolve(out, "/%2").is_some());
    }

    #[test]
    fn redirect_targets_stay_on_site() {
        assert_eq!(safe_next(Some("/blog/?page=2")), "/blog/?page=2");
        for bad in [
            "//evil.example",
            "https://evil.example",
            "\\\\evil",
            "/a\r\nSet-Cookie: x=1",
            "",
        ] {
            assert_eq!(safe_next(Some(bad)), "/", "{bad:?}");
        }
        assert_eq!(safe_next(None), "/");
    }

    #[test]
    fn toolbar_lists_themes_and_marks_active() {
        let js = toolbar_script(&["default".into(), "terminal".into()], "terminal");
        assert!(js.contains("T=[\"default\",\"terminal\"]") && js.contains("A=\"terminal\""));
        assert!(js.contains("/__meteorite/theme?name="));
    }

    #[test]
    fn links_never_point_at_the_wildcard_address() {
        assert_eq!(advertised_host("0.0.0.0"), "127.0.0.1");
        assert_eq!(advertised_host("::"), "[::1]");
        assert_eq!(advertised_host("127.0.0.1"), "127.0.0.1");
        assert_eq!(advertised_host("192.168.1.20"), "192.168.1.20");
        assert_eq!(advertised_host("::1"), "[::1]");
        assert_eq!(advertised_host("localhost"), "localhost");
    }

    #[test]
    fn preview_dir_is_outside_the_site_and_unique_per_root() {
        let a = preview_dir(Path::new("/srv/site-a"));
        let b = preview_dir(Path::new("/srv/site-b"));
        assert_ne!(a, b);
        assert_eq!(a, preview_dir(Path::new("/srv/site-a")));
        assert!(!a.starts_with("/srv/site-a"));
    }
}
