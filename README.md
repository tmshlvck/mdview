# Simple and fast MarkDown viewer in Rust

A console-based markdown viewer that serves files over HTTP with live reload. Opens in your browser automatically and watches for file changes.

## Features

- **Live reload**: Automatically updates when the markdown file changes using file system notifications
- **WebSocket-based push updates**: Real-time updates without page refresh (default mode)
- **Refresh interval mode**: Alternative polling-based refresh mode with configurable intervals
- **Random port assignment**: Automatically finds an available port
- **Cross-platform browser opening**: Works on Linux, macOS, and Windows
- **All markdown extensions enabled**: Tables, footnotes, strikethrough, task lists, smart punctuation, heading attributes
- **Multiple browser options**: Default system browser, Chrome, Firefox, Chromium with normal/incognito/private modes

## Installation

```bash
cargo install --path . --root ~/.local
```

## Usage

### Basic usage (WebSocket mode - default)
```bash
mdview README.md
```

### Refresh mode with custom interval
```bash
mdview --refresh 5 README.md  # Refresh every 5 seconds
```

### Custom port
```bash
mdview --port 8080 README.md
```

### Use specific browser
```bash
mdview --browser chrome-incognito README.md  # Chrome incognito window
mdview --browser firefox-private README.md   # Firefox private window
```

## How it works

1. **Console app**: No GUI dependencies, runs as a simple console application
2. **HTTP server**: Uses Axum to serve the markdown as HTML on localhost
3. **File watching**: Uses the `notify` crate with efficient file system notifications (inotify on Linux)
4. **Live updates**:
   - **Default**: WebSocket connection pushes updates to browser instantly
   - **Refresh mode**: JavaScript periodically refreshes the page at specified intervals
5. **Auto browser**: Opens your default browser automatically when starting

## Credits

* Proudly vibe-coded with Claude Sonnet 4 :-)
