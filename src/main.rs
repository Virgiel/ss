use async_std::task;
use colored::{ColoredString, Colorize};
use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc,
    },
};
use std::{io::Write, time};
use tide::{http::Mime, Body, Request, Response, StatusCode};

#[derive(argh::FromArgs)]
/// Launch a static website server in current dir
struct Args {
    /// serve built files
    #[argh(switch, short = 'o')]
    pub open: bool,
    /// the path to the source directory
    #[argh(option, short = 'd', default = "String::from(\".\")")]
    pub dir: String,
    /// the port for the server
    #[argh(option, short = 'p', default = "8080")]
    pub port: u16,
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
    let mut res = Response::new(StatusCode::Ok);
    let arg: String = req.param("path").unwrap_or("".into());
    let path = if arg == "" { "index.html" } else { &arg };
    let end = time::Instant::now();
    let time = end.duration_since(start).as_micros() as f64 / 1000.0;
    let dash = "-".bright_black().bold();
    let instant = chrono::Local::now().format("%T").to_string().purple();

    let (content, status) = match std::fs::read_to_string(dir.join(path)) {
        Ok(content) => (Some(content), StatusCode::Ok),
        Err(_err) => (None, StatusCode::NotFound),
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
    res.set_status(status);

    if let Some(content) = content {
        let mut content = content.clone();
        if path.ends_with(".html") {
            content.push_str(include_str!("./hot.html"));
        }
        let mut body = Body::from_string(content);

        if let Some(mime) = Mime::from_extension(String::from(path).split(".").last().unwrap_or(""))
        {
            body.set_mime(mime);
        }

        res.set_body(body);
    }

    async_std::future::ready(Ok(res))
}
