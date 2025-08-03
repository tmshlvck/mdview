# Simple and fast MarkDown viewer in Rust

MarkDown viewer that can be used as an aid in writing MD - just open it next to your editor in tiled mode.

## Build

Dioxus deps
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

cargo install dioxus-cli
```

## Test

```
cargo run -- README.md
```

## Credits

* Proudly vibe-coded with Claude Sonet 4 :-)
