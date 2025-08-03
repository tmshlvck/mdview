use clap::{Arg, Command};
use dioxus::prelude::*;
use pulldown_cmark::{html, CowStr, Event, LinkType, Options, Parser, Tag};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use url::Url;

// Configuration constants
const MIN_REFRESH_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct MarkdownConfig {
    enable_tables: bool,
    enable_footnotes: bool,
    enable_strikethrough: bool,
    enable_tasklists: bool,
    enable_smart_punctuation: bool,
    enable_heading_attributes: bool,
}

impl Default for MarkdownConfig {
    fn default() -> Self {
        Self {
            enable_tables: true,
            enable_footnotes: true,
            enable_strikethrough: true,
            enable_tasklists: true,
            enable_smart_punctuation: true,
            enable_heading_attributes: true,
        }
    }
}

impl MarkdownConfig {
    fn to_options(&self) -> Options {
        let mut options = Options::empty();
        if self.enable_tables {
            options.insert(Options::ENABLE_TABLES);
        }
        if self.enable_footnotes {
            options.insert(Options::ENABLE_FOOTNOTES);
        }
        if self.enable_strikethrough {
            options.insert(Options::ENABLE_STRIKETHROUGH);
        }
        if self.enable_tasklists {
            options.insert(Options::ENABLE_TASKLISTS);
        }
        if self.enable_smart_punctuation {
            options.insert(Options::ENABLE_SMART_PUNCTUATION);
        }
        if self.enable_heading_attributes {
            options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
        }
        options
    }
}

fn main() {
    let matches = Command::new("markdown-display")
        .version("1.0")
        .about("A Markdown display application with live preview")
        .arg(
            Arg::new("file")
                .help("The markdown file to display")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("convert")
                .long("convert")
                .short('c')
                .help("Convert to HTML and exit")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("output")
                .long("output")
                .short('o')
                .help("Output file for HTML conversion")
                .value_name("FILE"),
        )
        .arg(
            Arg::new("no-tables")
                .long("no-tables")
                .help("Disable table extensions")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-footnotes")
                .long("no-footnotes")
                .help("Disable footnote extensions")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-strikethrough")
                .long("no-strikethrough")
                .help("Disable strikethrough extensions")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-tasklists")
                .long("no-tasklists")
                .help("Disable task list extensions")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-smart-punctuation")
                .long("no-smart-punctuation")
                .help("Disable smart punctuation")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-heading-attributes")
                .long("no-heading-attributes")
                .help("Disable heading attributes")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    let file_path = PathBuf::from(matches.get_one::<String>("file").unwrap());

    if !file_path.exists() {
        eprintln!("Error: File '{}' does not exist", file_path.display());
        std::process::exit(1);
    }

    let mut config = MarkdownConfig::default();
    if matches.get_flag("no-tables") {
        config.enable_tables = false;
    }
    if matches.get_flag("no-footnotes") {
        config.enable_footnotes = false;
    }
    if matches.get_flag("no-strikethrough") {
        config.enable_strikethrough = false;
    }
    if matches.get_flag("no-tasklists") {
        config.enable_tasklists = false;
    }
    if matches.get_flag("no-smart-punctuation") {
        config.enable_smart_punctuation = false;
    }
    if matches.get_flag("no-heading-attributes") {
        config.enable_heading_attributes = false;
    }

    if matches.get_flag("convert") {
        // One-off HTML conversion
        let content = fs::read_to_string(&file_path).unwrap_or_else(|e| {
            eprintln!("Error reading file: {}", e);
            std::process::exit(1);
        });

        let html_content = markdown_to_html(&content, &file_path, &config);

        if let Some(output_file) = matches.get_one::<String>("output") {
            fs::write(output_file, html_content).unwrap_or_else(|e| {
                eprintln!("Error writing output file: {}", e);
                std::process::exit(1);
            });
            println!("HTML written to {}", output_file);
        } else {
            println!("{}", html_content);
        }
        return;
    }

    // Store config and file path in environment for the GUI app
    std::env::set_var("MDVIEW_FILE_PATH", file_path.to_string_lossy().as_ref());
    std::env::set_var("MDVIEW_CONFIG", serde_json::to_string(&config).unwrap_or_default());

    // Launch the GUI application with desktop config
    dioxus::LaunchBuilder::desktop()
        .with_cfg(dioxus::desktop::Config::new()
            .with_menu(None) // Disable the useless menu
            .with_window(dioxus::desktop::WindowBuilder::new()
                .with_title("Markdown Display")
                .with_resizable(true)))
        .launch(app);
}

fn app() -> Element {
    // Get the file path and config from environment variables set by main()
    let file_path = use_signal(|| {
        if let Ok(path_str) = std::env::var("MDVIEW_FILE_PATH") {
            PathBuf::from(path_str)
        } else {
            // Fallback to command line args if env var not set
            let args: Vec<String> = std::env::args().collect();
            if args.len() >= 2 {
                PathBuf::from(&args[1])
            } else {
                PathBuf::from("README.md")
            }
        }
    });

    let config = use_signal(|| {
        if let Ok(config_str) = std::env::var("MDVIEW_CONFIG") {
            serde_json::from_str(&config_str).unwrap_or_default()
        } else {
            MarkdownConfig::default()
        }
    });

    let mut content = use_signal(String::new);
    let mut last_modified = use_signal(|| None::<std::time::SystemTime>);

    // Load initial content
    use_effect(move || {
        let path = file_path();
        if let Ok(file_content) = std::fs::read_to_string(&path) {
            content.set(file_content);
            if let Ok(metadata) = std::fs::metadata(&path) {
                if let Ok(modified) = metadata.modified() {
                    last_modified.set(Some(modified));
                }
            }
        }
    });

    // Set up periodic file checking
    use_effect(move || {
        let mut interval = use_signal(|| Instant::now());

        spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;

                let now = Instant::now();
                if now.duration_since(interval.read().clone()) >= MIN_REFRESH_INTERVAL {
                    interval.set(now);

                    let path = file_path();
                    if let Ok(metadata) = std::fs::metadata(&path) {
                        if let Ok(modified) = metadata.modified() {
                            let should_update = match last_modified.read().as_ref() {
                                Some(last) => modified > *last,
                                None => true,
                            };

                            if should_update {
                                if let Ok(new_content) = std::fs::read_to_string(&path) {
                                    content.set(new_content);
                                    last_modified.set(Some(modified));
                                }
                            }
                        }
                    }
                }
            }
        });
    });

    let current_content = content.read();
    if current_content.is_empty() {
        rsx! {
            div {
                style: "width: 100vw; height: 100vh; display: flex; align-items: center; justify-content: center; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;",
                "Loading..."
            }
        }
    } else {
        let html_content = markdown_to_html(&current_content, &file_path(), &config());
        rsx! {
            div {
                style: "margin: 0; padding: 0; width: 100%; height: 100vh; overflow-y: auto; overflow-x: hidden;",
                div {
                    style: "max-width: 800px; margin: 0 auto; padding: 20px; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;",
                    dangerous_inner_html: "{html_content}"
                }
            }
        }
    }
}

fn markdown_to_html(content: &str, file_path: &Path, config: &MarkdownConfig) -> String {
    let options = config.to_options();
    let parser = Parser::new_ext(content, options);

    // Transform events to handle relative paths
    let events: Vec<Event> = parser
        .map(|event| transform_event(event, file_path))
        .collect();

    let mut html_output = String::new();
    html::push_html(&mut html_output, events.into_iter());

    format!(
        r#"<!DOCTYPE html>
<html style="margin: 0; padding: 0; height: 100%;">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <style>
        * {{
            box-sizing: border-box;
        }}

        html, body {{
            margin: 0;
            padding: 0;
            height: 100%;
            overflow: hidden;
        }}

        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            line-height: 1.6;
            color: #333;
        }}

        h1, h2, h3, h4, h5, h6 {{
            margin-top: 1.5em;
            margin-bottom: 0.5em;
            font-weight: 600;
        }}

        h1 {{ font-size: 2em; border-bottom: 1px solid #eee; padding-bottom: 0.3em; }}
        h2 {{ font-size: 1.5em; border-bottom: 1px solid #eee; padding-bottom: 0.3em; }}

        code {{
            background-color: #f6f8fa;
            padding: 2px 4px;
            border-radius: 3px;
            font-family: 'SF Mono', Consolas, 'Liberation Mono', Menlo, monospace;
            font-size: 0.9em;
        }}

        pre {{
            background-color: #f6f8fa;
            padding: 16px;
            border-radius: 6px;
            overflow-x: auto;
        }}

        pre code {{
            background: none;
            padding: 0;
        }}

        blockquote {{
            border-left: 4px solid #dfe2e5;
            padding-left: 16px;
            margin-left: 0;
            color: #6a737d;
        }}

        table {{
            border-collapse: collapse;
            width: 100%;
            margin: 1em 0;
        }}

        th, td {{
            border: 1px solid #dfe2e5;
            padding: 8px 12px;
            text-align: left;
        }}

        th {{
            background-color: #f6f8fa;
            font-weight: 600;
        }}

        img {{
            max-width: 100%;
            height: auto;
        }}

        .task-list-item {{
            list-style-type: none;
        }}

        .task-list-item input[type="checkbox"] {{
            margin-right: 0.5em;
        }}
    </style>
</head>
<body>
{}
</body>
</html>"#,
        html_output
    )
}

fn transform_event<'a>(event: Event<'a>, file_path: &Path) -> Event<'a> {
    match event {
        Event::Start(Tag::Image { dest_url, title, id, .. }) => {
            let transformed_url = transform_relative_path(&dest_url, file_path);
            Event::Start(Tag::Image {
                dest_url: CowStr::Boxed(transformed_url.into()),
                title,
                id,
                link_type: LinkType::Inline,
            })
        }
        Event::Start(Tag::Link { dest_url, title, id, .. }) => {
            let transformed_url = transform_relative_path(&dest_url, file_path);
            Event::Start(Tag::Link {
                dest_url: CowStr::Boxed(transformed_url.into()),
                title,
                id,
                link_type: LinkType::Inline,
            })
        }
        _ => event,
    }
}

fn transform_relative_path(url: &str, file_path: &Path) -> String {
    // Check if it's already an absolute URL
    if Url::parse(url).is_ok() {
        return url.to_string();
    }

    // Check if it's an absolute path
    if url.starts_with('/') {
        return url.to_string();
    }

    // Transform relative path
    if let Some(parent) = file_path.parent() {
        let full_path = parent.join(url);
        if let Ok(canonical) = full_path.canonicalize() {
            // Print canonical path for local files
            return canonical.display().to_string();
        } else {
            // Fallback: just join the paths
            return parent.join(url).display().to_string();
        }
    }

    url.to_string()
}

// Cargo.toml dependencies needed:
/*
[dependencies]
clap = { version = "4.0", features = ["derive"] }
dioxus = { version = "0.6", features = ["desktop"] }
pulldown-cmark = { version = "0.12", features = ["simd"] }
tokio = { version = "1.0", features = ["full"] }
url = "2.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
*/
