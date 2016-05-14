//!Server configuration and instance.

use std::borrow::ToOwned;
use std::time::Duration;
use std::thread::{self, JoinHandle};

use hyper;
use hyper::mime::Mime;

use filter::{ContextFilter, ResponseFilter};
use router::Router;

use HttpResult;

pub use self::instance::{ServerInstance, ServerLoop};
pub use self::config::{Host, Global, Scheme};
pub use crossbeam::{Scope, ScopedJoinHandle};

mod instance;
mod config;

///Used to set up and run a server.
///
///```no_run
///# extern crate rustful;
///# #[macro_use] extern crate log;
///# use std::error::Error;
///# use rustful::{Server, Handler, Context, Response};
///# #[derive(Default)]
///# struct R;
///# impl Handler for R {
///#     fn handle_request(&self, _context: Context, _response: Response) {}
///# }
///# fn main() {
///# let router = R;
///let server_result = Server {
///    host: 8080.into(),
///    handlers: router,
///    ..Server::default()
///}.run();
///
///if let Err(e) = server_result {
///    error!("could not start server: {}", e.description())
///}
///# }
///```
pub struct Server<R: Router> {
    ///One or several response handlers.
    pub handlers: R,

    ///A fallback handler for when none is found in `handlers`. Leaving this
    ///unspecified will cause an empty `404` response to be automatically sent
    ///instead.
    pub fallback_handler: Option<R::Handler>,

    ///The host address and port where the server will listen for requests.
    ///Default is `0.0.0.0:80`.
    pub host: Host,

    ///Use good old HTTP or the more secure HTTPS. Default is HTTP.
    pub scheme: Scheme,

    ///The number of threads to be used in the server thread pool. The default
    ///(`None`) will cause the server to optimistically use the formula
    ///`(num_cores * 5) / 4`.
    pub threads: Option<usize>,

    ///Enables or disables `keep-alive`. Default is `true` (enabled).
    pub keep_alive: bool,

    ///Timeout for idle connections. Default is 10 seconds.
    pub timeout: Duration,

    ///The maximal number of open sockets. Default is 4096.
    pub max_sockets: usize,

    ///The content of the server header. Default is `"rustful"`.
    pub server: String,

    ///The default media type. Default is `text/plain, charset: UTF-8`.
    pub content_type: Mime,

    ///Globally accessible data.
    pub global: Global,

    ///The context filter stack.
    pub context_filters: Vec<Box<ContextFilter>>,

    ///The response filter stack.
    pub response_filters: Vec<Box<ResponseFilter>>
}

impl<R: Router> Server<R> {
    ///Set up a new standard server. This can be useful when `handlers`
    ///doesn't implement `Default`:
    ///
    ///```no_run
    ///# use std::error::Error;
    ///# use rustful::{Server, Handler, Context, Response};
    ///let handler = |context: Context, response: Response| {
    ///    //...
    ///};
    ///
    ///let server_result = Server {
    ///    host: 8080.into(),
    ///    ..Server::new(handler)
    ///};
    ///```
    pub fn new(handlers: R) -> Server<R> {
        Server {
            handlers: handlers,
            fallback_handler: None,
            host: 80.into(),
            scheme: Scheme::Http,
            threads: None,
            keep_alive: true,
            timeout: Duration::from_secs(10),
            max_sockets: 4096,
            server: "rustful".to_owned(),
            content_type: Mime(
                hyper::mime::TopLevel::Text,
                hyper::mime::SubLevel::Html,
                vec![(hyper::mime::Attr::Charset, hyper::mime::Value::Utf8)]
            ),
            global: Global::default(),
            context_filters: Vec::new(),
            response_filters: Vec::new(),
        }
    }

    ///Run the server using scoped threads, blocking this thread until the
    ///server shuts down.
    pub fn run(self) -> HttpResult<()> {
        self.run_and_then(|_, _| ())
    }

    ///Run the server using scoped threads, and then do something within the
    ///same scope, before blocking this thread.
    ///
    ///```no_run
    ///# extern crate rustful;
    ///# #[macro_use] extern crate log;
    ///# use std::error::Error;
    ///# use rustful::{Server, Handler, Context, Response};
    ///# #[derive(Default)]
    ///# struct R;
    ///# impl Handler for R {
    ///#     fn handle_request(&self, _context: Context, _response: Response) {}
    ///# }
    ///# fn main() {
    ///# let router = R;
    ///# let port = 8080;
    ///let server_result = Server {
    ///    host: port.into(),
    ///    handlers: router,
    ///    ..Server::default()
    ///}.run_and_then(|_scope, instance| {
    ///    println!("listening on port {}", instance.addr.port());
    ///});
    ///
    ///if let Err(e) = server_result {
    ///    error!("could not start server: {}", e.description())
    ///}
    ///# }
    ///```
    pub fn run_and_then<F, T>(self, action: F) -> HttpResult<T> where
        F: FnOnce(&Scope, ServerInstance<ScopedJoinHandle<()>>) -> T,
    {
        ::crossbeam::scope(|scope| {
            ServerInstance::run(self, |s| scope.spawn(|| s.run())).map(|instance| action(scope, instance))
        })
    }

    ///Run the server using regular, detached threads.
    ///
    ///```no_run
    ///# extern crate rustful;
    ///# #[macro_use] extern crate log;
    ///# use std::error::Error;
    ///# use rustful::{Server, Handler, Context, Response};
    ///# #[derive(Default)]
    ///# struct R;
    ///# impl Handler for R {
    ///#     fn handle_request(&self, _context: Context, _response: Response) {}
    ///# }
    ///# fn main() {
    ///# let router = R;
    ///# let port = 8080;
    ///let server_result = Server {
    ///    host: port.into(),
    ///    handlers: router,
    ///    ..Server::default()
    ///}.run_detached();
    ///
    ///match server_result {
    ///    Ok(instance) => println!("listening on port {}", instance.addr.port()),
    ///    Err(e) => error!("could not start server: {}", e.description()),
    ///}
    ///# }
    ///```
    pub fn run_detached(self) -> HttpResult<ServerInstance<JoinHandle<()>>> where
        R: 'static,
        R::Handler: 'static,
    {
        ServerInstance::run(self, |s| thread::spawn(move || s.run() ))
    }

    ///Run the server using some custom threading method.
    pub fn run_in<F, H>(self, new_thread: F) -> HttpResult<ServerInstance<H>> where
        F: FnMut(ServerLoop<R>) -> H,
        H: ThreadHandle,
    {
        ServerInstance::run(self, new_thread)
    }
}

impl<R: Router + Default> Default for Server<R> {
    fn default() -> Server<R> {
        Server::new(R::default())
    }
}

///Common functionality for thread handles.
pub trait ThreadHandle {
    ///Wait for the thread to finish.
    fn join(self);
}

impl ThreadHandle for JoinHandle<()> {
    fn join(self) {
        self.join().expect("failed to join server thread");
    }
}

impl ThreadHandle for ScopedJoinHandle<()> {
    fn join(self) {
        self.join();
    }
}
