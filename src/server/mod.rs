//!Server configuration and instance.

use std::borrow::ToOwned;

use hyper;
use hyper::mime::Mime;

pub use hyper::server::Listening;

use filter::{ContextFilter, ResponseFilter};
use handler::HandleRequest;
use net::SslServer;

use HttpResult;

pub use self::instance::ServerInstance;
pub use self::config::{Host, Global, KeepAlive};

mod instance;
mod config;

///Used to set up and run a server.
///
///```no_run
///# use std::error::Error;
///# use rustful::{Server, CreateContent, Context, Response};
///# #[derive(Default)]
///# struct R;
///# impl CreateContent for R {
///#     type Output = ();
///#     fn create_content(&self, _context: &mut Context, _response: &Response) -> () {}
///# }
///# let router = R;
///let server_result = Server {
///    host: 8080.into(),
///    handlers: router,
///    ..Server::default()
///}.run();
///
///match server_result {
///    Ok(_server) => {},
///    Err(e) => println!("could not start server: {}", e.description())
///}
///```
pub struct Server<R> {
    ///One or several response handlers.
    pub handlers: R,

    ///The host address and port where the server will listen for requests.
    ///Default is `0.0.0.0:80`.
    pub host: Host,

    ///The number of threads to be used in the server thread pool. The default
    ///(`None`) will cause the server to optimistically use the formula
    ///`(num_cores * 5) / 4`.
    pub threads: Option<usize>,

    ///The server's `keep-alive` policy. Setting this to `Some(...)` will
    ///allow `keep-alive` connections with a timeout, and keeping it as `None`
    ///will force connections to close after each request. Default is `None`.
    pub keep_alive: Option<KeepAlive>,

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

impl<R: HandleRequest> Server<R> {
    ///Set up a new standard server. This can be useful when `handlers`
    ///doesn't implement `Default`:
    ///
    ///```no_run
    ///# use std::error::Error;
    ///# use rustful::{Server, Context, Response};
    ///let handler = |_: &mut Context| -> &'static str {
    ///    "Hello world!"
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
            host: 80.into(),
            threads: None,
            keep_alive: None,
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

    ///Start the server.
    pub fn run(self) -> HttpResult<Listening> {
        self.build().run()
    }

    ///Start the server with SSL.
    pub fn run_https<S: SslServer + Clone + Send + 'static>(self, ssl: S) -> HttpResult<Listening> {
        self.build().run_https(ssl)
    }

    ///Build a runnable instance of the server.
    pub fn build(self) -> ServerInstance<R> {
        ServerInstance::new(self)
    }
}

impl<R: Default + HandleRequest> Default for Server<R> {
    fn default() -> Server<R> {
        Server::new(R::default())
    }
}
