use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, Path as AxumPath, State},
    http::{StatusCode, HeaderMap, header::{CONTENT_TYPE, CACHE_CONTROL}},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use clap::{Arg, Command};
use futures_util::{SinkExt, StreamExt};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use pulldown_cmark::{html, CowStr, Event as MarkdownEvent, LinkType, Options, Parser, Tag};
use std::{
    collections::HashSet,
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::broadcast;
use tower_http::services::ServeDir;

#[derive(Clone, Debug)]
struct AppState {
    file_path: PathBuf,
    root_dir: PathBuf,
    reload_sender: broadcast::Sender<()>,
    refresh_interval: Option<u64>,
}

#[tokio::main]
async fn main() {
    let matches = Command::new("mdview")
        .version("0.1.1")
        .about("A fast markdown viewer with live reload")
        .arg(
            Arg::new("file")
                .help("The markdown file to display")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("port")
                .long("port")
                .short('p')
                .help("Port to serve on (random if not specified)")
                .value_name("PORT"),
        )
        .arg(
            Arg::new("refresh")
                .long("refresh")
                .short('r')
                .help("Enable refresh mode with interval in seconds")
                .value_name("SECONDS"),
        )
        .arg(
            Arg::new("browser")
                .long("browser")
                .short('b')
                .help("Browser to open (default, chrome, chrome-incognito, firefox, firefox-private, chromium, chromium-incognito)")
                .value_name("BROWSER")
                .default_value("default"),
        )
        .get_matches();

    let file_path = PathBuf::from(matches.get_one::<String>("file").unwrap());

    if !file_path.exists() {
        eprintln!("Error: File '{}' does not exist", file_path.display());
        std::process::exit(1);
    }

    let browser = matches.get_one::<String>("browser").unwrap().clone();

    let port: u16 = matches
        .get_one::<String>("port")
        .and_then(|p| p.parse().ok())
        .unwrap_or(0); // 0 means random port

    let refresh_interval = matches
        .get_one::<String>("refresh")
        .and_then(|s| s.parse().ok());

    let (reload_sender, _) = broadcast::channel(16);

    let root_dir = file_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    
    let state = AppState {
        file_path: file_path.clone(),
        root_dir,
        reload_sender: reload_sender.clone(),
        refresh_interval,
    };

    // Set up file watching
    let watched_files = Arc::new(tokio::sync::Mutex::new(HashSet::new()));
    let reload_sender_clone = reload_sender.clone();
    let watched_files_clone = watched_files.clone();

    tokio::spawn(async move {
        if let Err(e) = setup_file_watcher(file_path, reload_sender_clone, watched_files_clone).await {
            eprintln!("File watcher error: {}", e);
        }
    });

    // Create router
    let app = Router::new()
        .route("/", get(serve_markdown))
        .route("/ws", get(websocket_handler))
        .route("/md/*path", get(serve_linked_markdown))
        .route("/files/*path", get(serve_file))
        .nest_service("/static", ServeDir::new("static"))
        .with_state(state);

    // Start server
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let actual_addr = listener.local_addr().unwrap();

    println!("Serving markdown at http://localhost:{}", actual_addr.port());

    // Open browser
    let url = format!("http://localhost:{}", actual_addr.port());
    if let Err(e) = open_browser(&url, &browser) {
        eprintln!("Failed to open browser: {}", e);
        println!("Please open {} in your browser", url);
    }

    axum::serve(listener, app).await.unwrap();
}

async fn serve_markdown(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let content = fs::read_to_string(&state.file_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let html_content = markdown_to_html(&content, &state.file_path, state.refresh_interval);
    Ok(Html(html_content))
}

async fn serve_linked_markdown(
    AxumPath(path): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<Html<String>, StatusCode> {
    // Security: prevent directory traversal
    if path.contains("..") || path.contains("//") {
        return Err(StatusCode::BAD_REQUEST);
    }

    let file_path = state.root_dir.join(&path);
    
    // Ensure it's a markdown file
    if !file_path.extension().map_or(false, |ext| ext == "md" || ext == "markdown") {
        return Err(StatusCode::NOT_FOUND);
    }

    // Check if file exists
    if !file_path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    let content = fs::read_to_string(&file_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Render markdown without websocket/refresh functionality (only for main file)
    let html_content = markdown_to_html(&content, &file_path, None);
    Ok(Html(html_content))
}

async fn serve_file(
    AxumPath(path): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {
    // Security: prevent directory traversal
    if path.contains("..") || path.contains("//") {
        return Err(StatusCode::BAD_REQUEST);
    }

    let file_path = state.root_dir.join(&path);
    
    // Check if file exists
    if !file_path.exists() || !file_path.is_file() {
        return Err(StatusCode::NOT_FOUND);
    }

    let content = fs::read(&file_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mime_type = get_mime_type(&file_path);
    
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, mime_type.parse().unwrap());
    headers.insert(CACHE_CONTROL, "public, max-age=3600".parse().unwrap());

    Ok((headers, content).into_response())
}

fn get_mime_type(file_path: &Path) -> &'static str {
    match file_path.extension().and_then(|ext| ext.to_str()) {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("pdf") => "application/pdf",
        Some("txt") => "text/plain",
        Some("css") => "text/css",
        Some("js") => "application/javascript",
        Some("json") => "application/json",
        Some("xml") => "application/xml",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        _ => "application/octet-stream",
    }
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    ws.on_upgrade(|socket| handle_websocket(socket, state))
}

async fn handle_websocket(socket: WebSocket, state: AppState) {
    let mut rx = state.reload_sender.subscribe();
    let (sender, mut receiver) = socket.split();
    let sender = Arc::new(tokio::sync::Mutex::new(sender));

    // Handle incoming messages (ping/pong)
    let ping_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            if let Ok(Message::Pong(_)) = msg {
                // Client is alive
            }
        }
    });

    // Send reload notifications
    let reload_sender = sender.clone();
    let reload_task = tokio::spawn(async move {
        while let Ok(_) = rx.recv().await {
            let mut sender = reload_sender.lock().await;
            if sender.send(Message::Text("reload".to_string())).await.is_err() {
                break;
            }
        }
    });

    // Send periodic pings
    let mut ping_interval = tokio::time::interval(Duration::from_secs(30));
    let ping_sender = sender.clone();
    let ping_ping_task = tokio::spawn(async move {
        loop {
            ping_interval.tick().await;
            let mut sender = ping_sender.lock().await;
            if sender.send(Message::Ping(vec![])).await.is_err() {
                break;
            }
        }
    });

    // Wait for any task to complete
    tokio::select! {
        _ = ping_task => {},
        _ = reload_task => {},
        _ = ping_ping_task => {},
    }
}

async fn setup_file_watcher(
    file_path: PathBuf,
    reload_sender: broadcast::Sender<()>,
    _watched_files: Arc<tokio::sync::Mutex<HashSet<PathBuf>>>,
) -> notify::Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        },
        Config::default(),
    )?;

    watcher.watch(&file_path, RecursiveMode::NonRecursive)?;

    // Also watch the directory for new files
    if let Some(parent) = file_path.parent() {
        watcher.watch(parent, RecursiveMode::NonRecursive)?;
    }

    while let Some(event) = rx.recv().await {
        let paths: Vec<PathBuf> = event.paths;
        let should_reload = paths.iter().any(|path| {
            path == &file_path || path.extension().map_or(false, |ext| ext == "md" || ext == "markdown")
        });

        if should_reload {
            let _ = reload_sender.send(());
        }
    }

    Ok(())
}

fn markdown_to_html(content: &str, file_path: &Path, refresh_interval: Option<u64>) -> String {
    let parser = Parser::new_ext(content, Options::all());

    let root_dir = file_path.parent().unwrap_or(Path::new("."));
    
    // Transform events to handle relative paths
    let events: Vec<MarkdownEvent> = parser
        .map(|event| transform_event(event, file_path, root_dir))
        .collect();

    let mut html_output = String::new();
    html::push_html(&mut html_output, events.into_iter());

    let websocket_script = if refresh_interval.is_some() {
        format!(
            r#"
            <script>
                setInterval(() => {{
                    location.reload();
                }}, {}000);
            </script>
            "#,
            refresh_interval.unwrap()
        )
    } else {
        r#"
        <script>
            const ws = new WebSocket(`ws://${window.location.host}/ws`);
            ws.onmessage = function(event) {
                if (event.data === 'reload') {
                    location.reload();
                }
            };
            ws.onclose = function() {
                // Reconnect after a short delay
                setTimeout(() => {
                    location.reload();
                }, 1000);
            };
        </script>
        "#.to_string()
    };

    format!(
        r#"<!DOCTYPE html>
<html style="margin: 0; padding: 0; height: 100%;">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Markdown Viewer</title>
    <style>
        * {{
            box-sizing: border-box;
        }}

        html, body {{
            margin: 0;
            padding: 0;
            height: 100%;
        }}

        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            line-height: 1.6;
            color: #333;
            max-width: 800px;
            margin: 0 auto;
            padding: 20px;
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
{}
</body>
</html>"#,
        html_output, websocket_script
    )
}

fn transform_event<'a>(event: MarkdownEvent<'a>, file_path: &Path, _root_dir: &Path) -> MarkdownEvent<'a> {
    match event {
        MarkdownEvent::Start(Tag::Image {
            dest_url,
            title,
            id,
            ..
        }) => {
            let transformed_url = transform_relative_path(&dest_url, file_path);
            MarkdownEvent::Start(Tag::Image {
                dest_url: CowStr::Boxed(transformed_url.into()),
                title,
                id,
                link_type: LinkType::Inline,
            })
        }
        MarkdownEvent::Start(Tag::Link {
            dest_url,
            title,
            id,
            ..
        }) => {
            let transformed_url = transform_relative_path(&dest_url, file_path);
            MarkdownEvent::Start(Tag::Link {
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
    // Check if it's already an absolute URL or starts with http/https
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("//") {
        return url.to_string();
    }

    // Check if it's already a server path
    if url.starts_with('/') {
        return url.to_string();
    }

    // Transform relative path to HTTP URL
    if let Some(parent) = file_path.parent() {
        let full_path = parent.join(url);
        
        // Check if it's a markdown file
        if let Some(ext) = full_path.extension() {
            if ext == "md" || ext == "markdown" {
                return format!("/md/{}", url);
            }
        }
        
        // For other files (images, etc.), serve through /files/ route
        return format!("/files/{}", url);
    }

    url.to_string()
}

fn open_browser(url: &str, browser: &str) -> Result<(), Box<dyn std::error::Error>> {
    let result = match browser {
        "chrome" => open_chrome(url, false),
        "chrome-incognito" => open_chrome(url, true),
        "firefox" => open_firefox(url, false),
        "firefox-private" => open_firefox(url, true),
        "chromium" => open_chromium(url, false),
        "chromium-incognito" => open_chromium(url, true),
        "default" | _ => open_default_browser(url),
    };

    // If specific browser fails, fall back to default
    if result.is_err() && browser != "default" {
        eprintln!("Failed to open {}, falling back to default browser", browser);
        open_default_browser(url)
    } else {
        result
    }
}

fn open_chrome(url: &str, incognito: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = std::process::Command::new("google-chrome");
    cmd.arg("--new-window");
    if incognito {
        cmd.arg("--incognito");
    }
    cmd.arg(url);
    cmd.spawn()?;
    Ok(())
}

fn open_chromium(url: &str, incognito: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = std::process::Command::new("chromium");
    cmd.arg("--new-window");
    if incognito {
        cmd.arg("--incognito");
    }
    cmd.arg(url);
    cmd.spawn()?;
    Ok(())
}

fn open_firefox(url: &str, private: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = std::process::Command::new("firefox");
    if private {
        cmd.arg("--private-window");
    } else {
        cmd.arg("--new-window");
    }
    cmd.arg(url);
    cmd.spawn()?;
    Ok(())
}

fn open_default_browser(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd").args(["/c", "start", url]).spawn()?;
    }
    Ok(())
}