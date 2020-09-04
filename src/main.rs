use async_std::task;
use colored::Colorize;
use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc,
    },
};
use tide::{http::Mime, Body, Request, Response, StatusCode};

#[derive(argh::FromArgs)]
/// Launch a static website server in current dir
struct Args {
    /// serve built files
    #[argh(switch, short = 'o')]
    pub open: bool,
    /// the path to the source directory
    #[argh(option, default = "String::from(\".\")")]
    pub dir: String,
    // TODO: add port config
}

fn main() {
    let args: Args = argh::from_env();
    let open = args.open;
    let dir = PathBuf::from(args.dir);

    let version_counter = Arc::new(AtomicU16::new(0));
    let mut hotwatch = hotwatch::Hotwatch::new().expect("Failed to initialize watcher");
    {
        let counter = version_counter.clone();
        hotwatch
            .watch(dir.clone(), move |_| {
                counter.fetch_add(1, Ordering::Relaxed);
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
        if open {
            open::that("http://127.0.0.1:8080").unwrap();
        }
        println!("Online at http://127.0.0.1:8080");
        app.listen("127.0.0.1:8080").await.unwrap();
    })
}

fn serve_file(dir: &Path, req: Request<()>) -> impl Future<Output = tide::Result<Response>> {
    let mut res = Response::new(StatusCode::Ok);
    let arg: String = req.param("path").unwrap_or("".into());
    let path = if arg == "" { "index.html" } else { &arg };
    let mut content = match std::fs::read_to_string(dir.join(path)) {
        Ok(content) => {
            println!("{} /{}", "OK ".green(), arg);
            content
        }
        Err(err) => {
            println!("{} /{}", "ERR".red(), arg);
            return async_std::future::ready(Err(err.into()));
        }
    };
    if path.ends_with(".html") {
        content.push_str(include_str!("./hot.html"));
    }
    let mut body = Body::from_string(content);
    if let Some(mime) = Mime::from_extension(String::from(path).split(".").last().unwrap_or("")) {
        body.set_mime(mime);
    }
    res.set_body(body);
    async_std::future::ready(Ok(res))
}
