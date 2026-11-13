#![windows_subsystem = "windows"] // no console window

mod logo;

use std::borrow::Cow;
use std::path::PathBuf;

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy},
    window::{Window, WindowBuilder},
};
use wry::{
    dpi::{PhysicalPosition, PhysicalSize, Position, Size},
    http::Response,
    PageLoadEvent, Rect, WebView, WebViewBuilder,
};

/// Browser chrome (tabs + address bar) rendered by its own webview.
const CHROME_PAGE: &str = include_str!("chrome.html");
/// The start page shown in new tabs.
const START_PAGE: &str = include_str!("start_page.html");
/// The settings page (search engine + extensions), served at start://localhost/settings.
const SETTINGS_PAGE: &str = include_str!("settings_page.html");

/// Height of the chrome (tab strip + address bar), in logical pixels.
const CHROME_H: f64 = 88.0;

const START_URL: &str = "start://localhost/";
const UI_URL: &str = "ui://localhost/";

/// Commands sent from the chrome UI (HTML) to Rust (via IPC).
#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "lowercase")]
enum ChromeCmd {
    /// The chrome DOM finished loading — push full state.
    Ready,
    Navigate { url: String },
    Back,
    Forward,
    Reload,
    NewTab,
    CloseTab { id: u64 },
    SwitchTab { id: u64 },
}

/// Commands sent from content pages (currently the settings page) to Rust.
#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "lowercase")]
enum ContentCmd {
    /// The settings page loaded — push current settings.
    GetSettings,
    SaveSettings { settings: SettingsUpdate },
}

/// Settings payload sent from the settings page when saving.
#[derive(Debug, Deserialize)]
struct SettingsUpdate {
    search_engine: String,
    extensions: Vec<ExtToggle>,
}

#[derive(Debug, Deserialize)]
struct ExtToggle {
    file: String,
    enabled: bool,
}

/// Persisted user settings, stored as JSON in the data dir.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Settings {
    search_engine: String,
    extensions: Vec<ExtState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExtState {
    file: String,
    enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            search_engine: "ddg".to_string(),
            extensions: Vec::new(),
        }
    }
}

/// A userscript found in the extensions folder.
struct Extension {
    name: String,
    file: String,
    code: String,
    enabled: bool,
}

/// `%LOCALAPPDATA%\Rust Browser` — where settings and extensions live.
fn data_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("Rust Browser")
}

fn load_settings() -> Settings {
    let path = data_dir().join("settings.json");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_settings(s: &Settings) {
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let json = serde_json::to_string_pretty(s).unwrap_or_default();
    let _ = std::fs::write(dir.join("settings.json"), json);
}

/// The sample extension created on first run so the feature is testable.
const SAMPLE_EXT: &str = r#"// @name Hello Extension
// Sample Rust Browser extension: shows a toast on every page.
(() => {
  const run = () => {
    const t = document.createElement('div');
    t.textContent = '\u{1F9CA} Hello from my first extension!';
    Object.assign(t.style, {
      position: 'fixed', top: '12px', right: '12px', zIndex: '2147483647',
      background: 'linear-gradient(180deg,#38bdf8,#0288d1)', color: '#fff',
      padding: '9px 16px', borderRadius: '20px',
      font: '600 13px "Segoe UI", sans-serif',
      boxShadow: '0 4px 14px rgba(2,136,209,.4)',
      pointerEvents: 'none', opacity: '0',
      transition: 'opacity .4s, transform .4s', transform: 'translateY(-8px)'
    });
    (document.body || document.documentElement).appendChild(t);
    requestAnimationFrame(() => { t.style.opacity = '1'; t.style.transform = 'translateY(0)'; });
    setTimeout(() => {
      t.style.opacity = '0';
      setTimeout(() => t.remove(), 500);
    }, 4000);
  };
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', run, { once: true });
  } else {
    run();
  }
})();
"#;

/// Scan `%LOCALAPPDATA%\Rust Browser\extensions\*.js`, creating the
/// sample extension on first run. Enabled flags come from settings.
fn load_extensions(settings: &Settings) -> Vec<Extension> {
    let dir = data_dir().join("extensions");
    if !dir.exists() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("hello.user.js"), SAMPLE_EXT);
    }
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in rd.flatten() {
        let file = entry.file_name().to_string_lossy().to_string();
        if !file.ends_with(".js") {
            continue;
        }
        let Ok(code) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        // `// @name My Extension` header, else the file name.
        let name = code
            .lines()
            .find_map(|l| l.trim().strip_prefix("// @name").map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| file.clone());
        let enabled = settings
            .extensions
            .iter()
            .find(|e| e.file == file)
            .map(|e| e.enabled)
            .unwrap_or(true);
        out.push(Extension {
            name,
            file,
            code,
            enabled,
        });
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// Scripts to inject into newly created tabs (enabled extensions only).
fn enabled_scripts(exts: &[Extension]) -> Vec<String> {
    exts.iter().filter(|e| e.enabled).map(|e| e.code.clone()).collect()
}

/// Messages delivered to the main event loop from webview callbacks.
/// Handlers capture their tab's id and forward here, so all tab logic
/// lives in one place: the `run` closure.
enum Msg {
    PageLoaded { id: u64, url: String },
    TitleChanged { id: u64, title: String },
    Chrome(ChromeCmd),
    Content { id: u64, cmd: ContentCmd },
}

/// One browser tab: a content webview plus the state shown in the strip.
struct Tab {
    id: u64,
    title: String,
    url: String,
    webview: WebView,
}

impl Tab {
    /// Text shown in the tab strip / window title.
    fn display_title(&self) -> String {
        if self.title.trim().is_empty() {
            if self.url.starts_with("start://") || self.url.is_empty() {
                return "New Tab".to_string();
            }
            // No title yet — show a readable piece of the URL instead.
            let u = self
                .url
                .trim_start_matches("https://")
                .trim_start_matches("http://");
            let host = u.split('/').next().unwrap_or("");
            if host.is_empty() {
                "New Tab".to_string()
            } else {
                host.to_string()
            }
        } else {
            self.title.clone()
        }
    }
}

/// Turn whatever the user typed into a real URL:
/// "rust lang"   -> search with the configured engine
/// "example.com" -> https://example.com
/// "settings"    -> the built-in settings page
fn normalize_url(input: &str, engine: &str) -> String {
    let t = input.trim();
    if t.is_empty() {
        return START_URL.to_string();
    }
    if t.eq_ignore_ascii_case("settings") {
        return format!("{START_URL}settings");
    }
    let looks_like_url = !t.contains(' ') && t.contains('.') && !t.ends_with('.');
    if looks_like_url {
        if t.starts_with("http://") || t.starts_with("https://") {
            t.to_string()
        } else {
            format!("https://{t}")
        }
    } else {
        let q = utf8_percent_encode(t, NON_ALPHANUMERIC);
        match engine {
            "google" => format!("https://www.google.com/search?q={q}"),
            "bing" => format!("https://www.bing.com/search?q={q}"),
            _ => format!("https://lite.duckduckgo.com/lite/?q={q}"),
        }
    }
}

/// A rectangle in physical pixels, the unit `set_bounds` expects here.
fn rect(x: i32, y: i32, w: u32, h: u32) -> Rect {
    Rect {
        position: Position::Physical(PhysicalPosition::new(x, y)),
        size: Size::Physical(PhysicalSize::new(w, h)),
    }
}

/// Inject a call into the chrome page (errors ignored: it may not be
/// loaded yet, and a full resync happens when it reports `ready`).
fn chrome_js(chrome: &WebView, call: String) {
    let _ = chrome.evaluate_script(&call);
}

/// Create a content webview for a tab. The handlers capture this tab's
/// id so events can be routed back to the right tab.
fn create_tab(
    window: &Window,
    id: u64,
    proxy: &EventLoopProxy<Msg>,
    bounds: Rect,
    logo_png: &[u8],
    scripts: &[String],
) -> WebView {
    let logo_png = logo_png.to_vec();
    let proxy_loaded = proxy.clone();
    let mut builder = WebViewBuilder::new()
        .with_url(START_URL)
        .with_bounds(bounds)
        .with_custom_protocol("start".into(), move |_id, req| {
            match req.uri().path() {
                "/logo.png" => Response::builder()
                    .header("Content-Type", "image/png")
                    .body(Cow::Owned(logo_png.clone()))
                    .unwrap(),
                "/settings" => Response::builder()
                    .header("Content-Type", "text/html; charset=utf-8")
                    .body(Cow::Borrowed(SETTINGS_PAGE.as_bytes()))
                    .unwrap(),
                _ => Response::builder()
                    .header("Content-Type", "text/html; charset=utf-8")
                    .body(Cow::Borrowed(START_PAGE.as_bytes()))
                    .unwrap(),
            }
        })
        .with_ipc_handler({
            let proxy = proxy.clone();
            move |req| {
                let body = req.into_body();
                if let Ok(cmd) = serde_json::from_str::<ContentCmd>(&body) {
                    let _ = proxy.send_event(Msg::Content { id, cmd });
                }
            }
        })
        .with_on_page_load_handler(move |event, url| {
            if matches!(event, PageLoadEvent::Finished) {
                let _ = proxy_loaded.send_event(Msg::PageLoaded { id, url });
            }
        })
        .with_document_title_changed_handler({
            let proxy = proxy.clone();
            move |title| {
                let _ = proxy.send_event(Msg::TitleChanged { id, title });
            }
        });

    // Enabled userscripts run before the page's own scripts.
    for script in scripts {
        builder = builder.with_initialization_script(script);
    }

    builder
        .build_as_child(window)
        .expect("failed to create tab webview")
}

/// Rebuild the tab strip in the chrome page.
fn sync_tabs(chrome: &WebView, tabs: &[Tab], active: usize) {
    let list: Vec<_> = tabs
        .iter()
        .enumerate()
        .map(|(i, t)| {
            json!({
                "id": t.id,
                "title": t.display_title(),
                "active": i == active,
            })
        })
        .collect();
    let j = serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string());
    chrome_js(chrome, format!("renderTabs({j})"));
}

/// Update address bar, nav buttons and window title for the active tab.
fn sync_active(chrome: &WebView, window: &Window, tabs: &[Tab], active: usize) {
    let t = &tabs[active];
    let jurl = serde_json::to_string(&t.url).unwrap_or_default();
    chrome_js(chrome, format!("updateUrl({jurl})"));
    let back = t.webview.can_go_back().unwrap_or(false);
    let fwd = t.webview.can_go_forward().unwrap_or(false);
    chrome_js(chrome, format!("updateNav({back},{fwd})"));
    let title = t.display_title();
    window.set_title(if title == "New Tab" {
        "Rust Browser"
    } else {
        &title
    });
}

fn main() {
    // Dev helper: `browser-rs.exe --dump-logo` writes a preview PNG and exits.
    if std::env::args().any(|a| a == "--dump-logo") {
        let rgba = logo::draw_rgba();
        std::fs::write("logo_preview.png", logo::png_bytes(&rgba)).expect("write preview");
        return;
    }

    // User events let webview callbacks notify the main event loop.
    let event_loop: EventLoop<Msg> = EventLoopBuilder::<Msg>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    // Logo: drawn in code, used as the window/taskbar icon and served
    // to pages as logo.png via the custom protocols below.
    let icon_rgba = logo::draw_rgba();
    let logo_png = logo::png_bytes(&icon_rgba);
    let window_icon = tao::window::Icon::from_rgba(icon_rgba, logo::OUT as u32, logo::OUT as u32).ok();

    let window = WindowBuilder::new()
        .with_title("Rust Browser")
        .with_window_icon(window_icon)
        .with_inner_size(tao::dpi::LogicalSize::new(1100.0, 750.0))
        .build(&event_loop)
        .expect("failed to create window");

    // Persisted settings + userscripts from the extensions folder.
    let mut settings = load_settings();
    let mut exts = load_extensions(&settings);

    let inner = window.inner_size();
    let scale = window.scale_factor();
    let chrome_h = (CHROME_H * scale).round() as u32;
    let mut content_bounds = rect(
        0,
        chrome_h as i32,
        inner.width,
        inner.height.saturating_sub(chrome_h),
    );

    // First tab.
    let scripts = enabled_scripts(&exts);
    let first = create_tab(&window, 0, &proxy, content_bounds, &logo_png, &scripts);
    let mut tabs = vec![Tab {
        id: 0,
        title: String::new(),
        url: START_URL.to_string(),
        webview: first,
    }];
    let mut active: usize = 0;
    let mut next_id: u64 = 1;

    // --- Chrome webview (tab strip + address bar) ---
    let chrome = WebViewBuilder::new()
        .with_url(UI_URL)
        .with_bounds(rect(0, 0, inner.width, chrome_h))
        .with_custom_protocol("ui".into(), {
            let logo_png = logo_png.clone();
            move |_id, req| {
                if req.uri().path() == "/logo.png" {
                    Response::builder()
                        .header("Content-Type", "image/png")
                        .body(Cow::Owned(logo_png.clone()))
                        .unwrap()
                } else {
                    Response::builder()
                        .header("Content-Type", "text/html; charset=utf-8")
                        .body(Cow::Borrowed(CHROME_PAGE.as_bytes()))
                        .unwrap()
                }
            }
        })
        .with_ipc_handler({
            let proxy = proxy.clone();
            move |req| {
                let body = req.into_body();
                if let Ok(cmd) = serde_json::from_str::<ChromeCmd>(&body) {
                    let _ = proxy.send_event(Msg::Chrome(cmd));
                }
            }
        })
        .build_as_child(&window)
        .expect("failed to create chrome webview");

    let _ = tabs[active].webview.focus();

    // --- Main event loop ---
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::UserEvent(msg) => match msg {
                Msg::PageLoaded { id, url } => {
                    if let Some(i) = tabs.iter().position(|t| t.id == id) {
                        tabs[i].url = url;
                        if i == active {
                            sync_active(&chrome, &window, &tabs, active);
                        }
                        sync_tabs(&chrome, &tabs, active);
                    }
                }
                Msg::TitleChanged { id, title } => {
                    if let Some(i) = tabs.iter().position(|t| t.id == id) {
                        if !title.trim().is_empty() {
                            tabs[i].title = title;
                            if i == active {
                                sync_active(&chrome, &window, &tabs, active);
                            }
                            sync_tabs(&chrome, &tabs, active);
                        }
                    }
                }
                Msg::Chrome(cmd) => match cmd {
                    ChromeCmd::Ready => {
                        // The chrome DOM exists now: push full state.
                        if let Ok(u) = tabs[active].webview.url() {
                            tabs[active].url = u;
                        }
                        sync_tabs(&chrome, &tabs, active);
                        sync_active(&chrome, &window, &tabs, active);
                    }
                    ChromeCmd::Navigate { url } => {
                        let target = normalize_url(&url, &settings.search_engine);
                        let _ = tabs[active].webview.load_url(&target);
                    }
                    ChromeCmd::Back => {
                        let _ = tabs[active].webview.go_back();
                    }
                    ChromeCmd::Forward => {
                        let _ = tabs[active].webview.go_forward();
                    }
                    ChromeCmd::Reload => {
                        let _ = tabs[active].webview.reload();
                    }
                    ChromeCmd::NewTab => {
                        let id = next_id;
                        next_id += 1;
                        let scripts = enabled_scripts(&exts);
                        let wv = create_tab(&window, id, &proxy, content_bounds, &logo_png, &scripts);
                        let old = active;
                        tabs.push(Tab {
                            id,
                            title: String::new(),
                            url: START_URL.to_string(),
                            webview: wv,
                        });
                        active = tabs.len() - 1;
                        let _ = tabs[old].webview.set_visible(false);
                        let _ = tabs[active].webview.focus();
                        sync_tabs(&chrome, &tabs, active);
                        sync_active(&chrome, &window, &tabs, active);
                    }
                    ChromeCmd::CloseTab { id } => {
                        if let Some(i) = tabs.iter().position(|t| t.id == id) {
                            if tabs.len() == 1 {
                                // Closing the last tab closes the window.
                                *control_flow = ControlFlow::Exit;
                            } else {
                                tabs.remove(i); // drops the webview
                                if i < active {
                                    active -= 1;
                                } else if i == active {
                                    active = i.min(tabs.len() - 1);
                                    let _ = tabs[active].webview.focus();
                                }
                                let _ = tabs[active].webview.set_visible(true);
                                sync_tabs(&chrome, &tabs, active);
                                sync_active(&chrome, &window, &tabs, active);
                            }
                        }
                    }
                    ChromeCmd::SwitchTab { id } => {
                        if let Some(i) = tabs.iter().position(|t| t.id == id) {
                            if i != active {
                                let _ = tabs[active].webview.set_visible(false);
                                active = i;
                                let _ = tabs[active].webview.set_visible(true);
                                let _ = tabs[active].webview.focus();
                                sync_tabs(&chrome, &tabs, active);
                                sync_active(&chrome, &window, &tabs, active);
                            }
                        }
                    }
                },
                Msg::Content { id, cmd } => match cmd {
                    ContentCmd::GetSettings => {
                        // The settings page loaded — push current state to it.
                        if let Some(tab) = tabs.iter().find(|t| t.id == id) {
                            let payload = json!({
                                "search_engine": settings.search_engine,
                                "extensions": exts.iter().map(|e| json!({
                                    "name": e.name,
                                    "file": e.file,
                                    "enabled": e.enabled,
                                })).collect::<Vec<_>>(),
                            });
                            let j = payload.to_string();
                            let _ = tab.webview.evaluate_script(&format!("applySettings({j})"));
                        }
                    }
                    ContentCmd::SaveSettings { settings: upd } => {
                        settings.search_engine = upd.search_engine;
                        for t in upd.extensions {
                            if let Some(e) = exts.iter_mut().find(|e| e.file == t.file) {
                                e.enabled = t.enabled;
                            }
                        }
                        settings.extensions = exts
                            .iter()
                            .map(|e| ExtState {
                                file: e.file.clone(),
                                enabled: e.enabled,
                            })
                            .collect();
                        save_settings(&settings);
                        if let Some(tab) = tabs.iter().find(|t| t.id == id) {
                            let _ = tab.webview.evaluate_script("settingsSaved()");
                        }
                    }
                },
            },

            // Keep chrome + every tab stacked correctly when resizing.
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => {
                content_bounds = rect(
                    0,
                    chrome_h as i32,
                    size.width,
                    size.height.saturating_sub(chrome_h),
                );
                let _ = chrome.set_bounds(rect(0, 0, size.width, chrome_h));
                for t in &tabs {
                    let _ = t.webview.set_bounds(content_bounds);
                }
            }

            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }

            _ => {}
        }
    });
}
