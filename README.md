# Rust Browser 🧊🦀

A fast, light web browser built in Rust — powered by WebView2 (Edge Chromium), no Electron, no bloat.

![logo](dist/rust_browser.ico)

## Features

- **Tabs** — create, close, switch; per-tab titles and history
- **Address bar** — type a URL *or* a search; smart detection, back/forward/reload
- **Icy frost UI** — light glassmorphism theme, custom ice-crab logo drawn in code
- **Settings page** — search engine (DuckDuckGo Lite / Google / Bing), persisted to `%LOCALAPPDATA%\Rust Browser\settings.json`
- **Extensions** — lightweight userscript system: drop a `.js` file into `%LOCALAPPDATA%\Rust Browser\extensions`, toggle it in Settings, restart
- **No console window** — proper GUI app (`windows_subsystem = "windows"`)
- **Tiny footprint** — ~1.1 MB exe, ~60 MB RAM idle; runs happily on a 2 GB laptop

## Building

Prerequisites: [Rust](https://rustup.rs), VS Build Tools (C++ workload), Windows 10/11 with WebView2 (preinstalled on Win 11).

```bat
cargo build --release
```

Run: `target\release\browser-rs.exe`

## Installer

Install [Inno Setup 6](https://jrsoftware.org/isdl.php6), then:

```bat
"%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe" installer.iss
```

Produces `dist\RustBrowserSetup.exe` (user-level install, no admin needed).

## Extensions

Any `.js` file in `%LOCALAPPDATA%\Rust Browser\extensions` becomes an extension.
Name it with a header comment:

```js
// @name My Extension
// your code here — injected before every page loads
```

A sample extension (`hello.user.js`) is created on first run.

## Roadmap

1. ✅ Browser (tabs, address bar, settings, extensions, installer)
2. 🎮 Minecraft-like game in Rust ([Bevy](https://bevyengine.org))
3. 🛠️ Own browser engine (HTML/CSS/JS rendering + real Chrome extension support)

## License

MIT
