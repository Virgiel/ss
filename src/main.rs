use async_std::task;
use colored::{ColoredString, Colorize};
use mimalloc::MiMalloc;
use std::time;
use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc,
    },
};
use tide::{http::Mime, Body, Request, Response, StatusCode};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[derive(argh::FromArgs)]
/// Launch a static website server in current dir
struct Args {
    /// serve built files
    #[argh(switch, short = 'o')]
    pub open: bool,
    /// the port for the server
    #[argh(option, short = 'p', default = "8080")]
    pub port: u16,
    /// the path to the source directory
    #[argh(positional, default = "String::from(\".\")")]
    pub dir: String,
}

fn main() {
    let args: Args = argh::from_env();
    let open = args.open;
    let port = args.port;
    let dir = PathBuf::from(args.dir);

    let version_counter = Arc::new(AtomicU16::new(0));
    let mut hotwatch = hotwatch::Hotwatch::new().expect("Failed to initialize watcher");
    {
        let counter = version_counter.clone();
        hotwatch
            .watch(dir.clone(), move |_| {
                counter.fetch_add(1, Ordering::Relaxed);
                println!(" - {}", "Reload".white().bold());
            })
            .expect("Failed to watch source directory");
    }
    task::block_on(async move {
        let mut app = tide::new();
        {
            let dir = dir.clone();
            app.at("/")
                .get(move |req: Request<_>| serve_file(&dir, req));
        }
        {
            let dir = dir.clone();
            app.at("/*path")
                .get(move |req: Request<_>| serve_file(&dir, req));
        }
        app.at("/hot").get(move |_| {
            let version = version_counter.load(Ordering::Relaxed);
            async move { Ok(format!("{}", version)) }
        });
        let address = format!("127.0.0.1:{}", port);
        let html_address = format!("http://{}", address);
        if open {
            open::that(&html_address).unwrap();
        }
        println!(" - {}: {}", "Open".white().bold(), html_address);
        app.listen(address).await.unwrap();
    })
}

fn format_result_code(status: StatusCode) -> ColoredString {
    if status.is_client_error() || status.is_server_error() {
        format!("{}", status).red()
    } else if status.is_redirection() || status.is_informational() {
        format!("{}", status).yellow()
    } else {
        format!("{}", status).green()
    }
}

fn serve_file(dir: &Path, req: Request<()>) -> impl Future<Output = tide::Result<Response>> {
    let start = time::Instant::now();
    let arg = req.param("path").unwrap_or("");
    let mut path = PathBuf::from(arg);
    let end = time::Instant::now();
    let time = end.duration_since(start).as_micros() as f64 / 1000.0;
    let dash = "-".bright_black().bold();
    let instant = chrono::Local::now().format("%T").to_string().purple();

    let (content, status) = match std::fs::read(dir.join(&path)) {
        Ok(content) => (Some(content), StatusCode::Ok),
        Err(_err) => {
            path.push("index.html");
            match std::fs::read(dir.join(&path)) {
            Ok(content) => (Some(content), StatusCode::Ok),
            Err(_err) => (None, StatusCode::NotFound),
        }
    },
    };
    println!(
        "[{}] {} {} {}ms {} /{}",
        instant,
        format_result_code(status),
        dash,
        time,
        dash,
        arg
    );
    let mut res = Response::new(status);

    if let Some(content) = content {
        let mut body = if let Ok(mut str) = String::from_utf8(content.clone()) {
            if path.ends_with(".html") {
                str.push_str(include_str!("./hot.html"));
            }
            Body::from_string(str)
        } else {
            Body::from_bytes(content)
        };

        if let Some(mime) = Mime::from_extension(path.extension().unwrap_or_default().to_str().unwrap_or_default())
        {
            body.set_mime(mime);
        }

        res.set_body(body);
    }

    async_std::future::ready(Ok(res))
}
