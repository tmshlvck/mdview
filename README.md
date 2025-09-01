# Simple and fast MarkDown viewer in Rust

MarkDown viewer that can be used as an aid in writing MD - just open it next to your editor in tiled mode. Features live reload and search functionality.

## Features

- **Live reload**: Automatically updates when the markdown file changes
- **Search functionality** (Ctrl+F): Find text within the document with:
  - Case-insensitive search
  - Navigate between results with < and > buttons
  - Visual highlighting of all matches
  - Current match highlighted in a different color

## Build

### Dioxus deps -- Ubuntu
```
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

### Dioxus deps -- Fedora
```
sudo dnf install webkit2gtk4.1-devel \
  openssl-devel \
  curl \
  wget \
  file \
  libappindicator-gtk3-devel \
  librsvg2-devel \
  libxdo-devel
sudo dnf group install "c-development"
```

## Installation and usage example

```
cargo install dioxus-cli
cargo install --path . --root ~/.local
```

Test on this `README.md`:

```
mdview README.md
```

## Credits

* Proudly vibe-coded with Claude Sonet 4 :-)
