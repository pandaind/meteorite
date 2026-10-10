//! `meteorite serve`: static file server with file watching and live reload.

use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use meteorite_core::build::{BuildOptions, Builder};
use meteorite_core::Error;
use notify::{RecursiveMode, Watcher};
use tokio::sync::broadcast;

const WS_PATH: &str = "/__meteorite/ws";

const RELOAD_SCRIPT: &str = "<script>(function(){var s=new WebSocket((location.protocol==='https:'?'wss://':'ws://')+location.host+'/__meteorite/ws');s.onmessage=function(){location.reload()};s.onclose=function(){setTimeout(function(){location.reload()},1000)}})();</script>";

struct AppState {
    out_dir: PathBuf,
    reload: broadcast::Sender<()>,
}

pub fn run(
    root: &Path,
    mut opts: BuildOptions,
    bind: &str,
    port: u16,
    open: bool,
) -> Result<ExitCode, Error> {
    let origin = format!("http://{bind}:{port}");
    opts.base_url = Some(format!("{origin}/"));

    let mut builder = Builder::new(root);
    let report = builder.build(&opts)?;
    println!(
        "Built {} page(s) in {} ms",
        report.pages,
        report.elapsed.as_millis()
    );
    for w in &report.warnings {
        eprintln!("warning: {w}");
    }
    let out_dir = report.output.clone();

    let (reload_tx, _) = broadcast::channel::<()>(16);
    let (event_tx, event_rx) = mpsc::channel::<()>();

    // Watch the source directories that influence the output.
    let root_abs = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let out_abs = out_dir.canonicalize().unwrap_or_else(|_| out_dir.clone());
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
    std::thread::spawn(move || {
        while event_rx.recv().is_ok() {
            while event_rx.recv_timeout(Duration::from_millis(120)).is_ok() {}
            match builder.build(&opts) {
                Ok(r) => {
                    println!(
                        "Rebuilt in {} ms ({} written, {} unchanged)",
                        r.elapsed.as_millis(),
                        r.files_written,
                        r.files_unchanged
                    );
                    for w in &r.warnings {
                        eprintln!("warning: {w}");
                    }
                    let _ = tx.send(());
                }
                Err(e) => eprintln!("{:?}", miette::Report::new(e)),
            }
        }
    });

    let state = Arc::new(AppState {
        out_dir,
        reload: reload_tx,
    });
    let addr = format!("{bind}:{port}");
    let rt = tokio::runtime::Runtime::new().map_err(|e| Error::Msg(e.to_string()))?;
    rt.block_on(async move {
        let app = Router::new()
            .route(WS_PATH, get(ws_handler))
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

fn with_reload(html: Vec<u8>) -> Vec<u8> {
    let text = String::from_utf8_lossy(&html).into_owned();
    match text.rfind("</body>") {
        Some(at) => format!("{}{}{}", &text[..at], RELOAD_SCRIPT, &text[at..]).into_bytes(),
        None => format!("{text}{RELOAD_SCRIPT}").into_bytes(),
    }
}

async fn file_handler(State(state): State<Arc<AppState>>, uri: Uri) -> Response {
    let Some(path) = resolve(&state.out_dir, uri.path()) else {
        return (StatusCode::BAD_REQUEST, "bad path").into_response();
    };
    let (path, status) = if path.is_file() {
        (path, StatusCode::OK)
    } else {
        let not_found = state.out_dir.join("404.html");
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
                with_reload(bytes)
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
    }
}
