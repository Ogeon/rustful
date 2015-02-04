//!Server configuration and instance.

#![stable]

use std;
use std::collections::HashMap;
use std::sync::RwLock;
use std::old_io::net::ip::{IpAddr, Ipv4Addr, Port};
use std::borrow::ToOwned;

use time::{self, Timespec};

use url::percent_encoding::lossy_utf8_percent_decode;

use hyper;
use hyper::server::Handler as HyperHandler;
use hyper::header::{Headers, Date, ContentType, ContentLength};
use hyper::mime::Mime;
use hyper::uri::RequestUri;

pub use hyper::server::Listening;

use StatusCode;

use context::{self, Context};
use plugin::{ContextPlugin, ContextAction, ResponsePlugin};
use router::Router;
use cache::Cache;
use handler::Handler;
use response::Response;
use Protocol;

use utils;

///Used to build and run a server.
///
///Each field has a corresponding modifier method and
///calls to these methods can be chained for quick setup.
///
///```no_run
///# use std::error::Error;
///# use rustful::{Server, Handler, Context, Response};
///# struct R;
///# impl Handler for R {
///#     type Cache = ();
///#     fn handle_request(&self, _context: Context, _response: Response) {}
///# }
///# let router = R;
///let server_result = Server::new().port(8080).handlers(router).run();
///
///match server_result {
///    Ok(_server) => {},
///    Err(e) => println!("could not start server: {}", e.description())
///}
///```
pub struct Server<R, C> {
    ///One or several response handlers.
    pub handlers: R,

    ///The port where the server will listen for requests.
    pub port: Port,

    ///The host address where the server will listen for requests.
    pub host: IpAddr,

    ///The type of HTTP protocol.
    pub protocol: Protocol,

    ///The number of threads to be used in the server task pool.
    ///The default (`None`) is based on recommendations from the system.
    pub threads: Option<usize>,

    ///The content of the server header.
    pub server: String,

    ///The default media type.
    pub content_type: Mime,

    ///A structure where resources are cached.
    pub cache: C,

    ///How often the cache should be cleaned. Measured in seconds.
    pub cache_clean_interval: Option<i64>,

    ///The context plugin stack.
    #[unstable]
    pub context_plugins: Vec<Box<ContextPlugin<Cache=C> + Send + Sync>>,

    ///The response plugin stack.
    #[unstable]
    pub response_plugins: Vec<Box<ResponsePlugin + Send + Sync>>
}

impl Server<(), ()> {
    ///Create a new empty server which will listen on host address `0.0.0.0` and port `80`.
    pub fn new() -> Server<(), ()> {
        Server::with_cache(())
    }
}

impl<C: Cache> Server<(), C> {
    ///Create a new empty server with a cache.
    pub fn with_cache(cache: C) -> Server<(), C> {
        Server {
            handlers: (),
            port: 80,
            host: Ipv4Addr(0, 0, 0, 0),
            protocol: Protocol::Http,
            threads: None,
            server: "rustful".to_owned(),
            content_type: Mime(
                hyper::mime::TopLevel::Text,
                hyper::mime::SubLevel::Plain,
                vec![(hyper::mime::Attr::Charset, hyper::mime::Value::Utf8)]
            ),
            cache: cache,
            cache_clean_interval: None,
            context_plugins: Vec::new(),
            response_plugins: Vec::new()
        }
    }
}

impl<R, C> Server<R, C> {
    ///Set request handlers.
    pub fn handlers<NewRouter: Router<Handler=H>, H: Handler<Cache=C>>(self, handlers: NewRouter) -> Server<NewRouter, C> {
        Server {
            handlers: handlers,
            port: self.port,
            host: self.host,
            protocol: self.protocol,
            threads: self.threads,
            server: self.server,
            content_type: self.content_type,
            cache: self.cache,
            cache_clean_interval: self.cache_clean_interval,
            context_plugins: self.context_plugins,
            response_plugins: self.response_plugins,
        }
    }

    ///Change the port. Default is `80`.
    pub fn port(mut self, port: Port) -> Server<R, C> {
        self.port = port;
        self
    }

    ///Change the host address. Default is `0.0.0.0`.
    pub fn host(mut self, host: IpAddr) -> Server<R, C> {
        self.host = host;
        self
    }

    ///Change the protocol to HTTPS.
    pub fn https(mut self, cert: Path, key: Path) -> Server<R, C> {
        self.protocol = Protocol::Https {
            cert: cert,
            key: key
        };
        self
    }

    ///Set the number of threads to be used in the server task pool.
    ///
    ///Passing `None` will set it to the default number of threads,
    ///based on recommendations from the system.
    pub fn threads(mut self, threads: Option<usize>) -> Server<R, C> {
        self.threads = threads;
        self
    }

    ///Set the minimal number of seconds between each cache clean.
    ///
    ///Passing `None` disables cache cleaning.
    pub fn cache_clean_interval(mut self, interval: Option<u32>) -> Server<R, C> {
        self.cache_clean_interval = interval.map(|i| i as i64);
        self
    }

    ///Change the server response header. Default is `rustful`.
    pub fn server_name(mut self, name: String) -> Server<R, C> {
        self.server = name;
        self
    }

    ///Change the default content type. Default is `text/plain`.
    pub fn content_type(mut self, content_type: Mime) -> Server<R, C> {
        self.content_type = content_type;
        self
    }

    ///Add a context plugin to the plugin stack.
    #[unstable]
    pub fn with_context_plugin<P: ContextPlugin<Cache=C> + Send + Sync>(mut self, plugin: P) ->  Server<R, C> {
        self.context_plugins.push(Box::new(plugin) as Box<ContextPlugin<Cache=C> + Send + Sync>);
        self
    }

    ///Add a response plugin to the plugin stack.
    #[unstable]
    pub fn with_response_plugin<P: ResponsePlugin + Send + Sync>(mut self, plugin: P) ->  Server<R, C> {
        self.response_plugins.push(Box::new(plugin) as Box<ResponsePlugin + Send + Sync>);
        self
    }
}

impl<R, H, C> Server<R, C>
    where
    R: Router<Handler=H> + Send + Sync,
    H: Handler<Cache=C> + Send + Sync,
    C: Cache + Send + Sync
{
    ///Start the server.
    pub fn run(self) -> hyper::HttpResult<Listening> {
        let threads = self.threads;
        let (server, protocol) = self.build();
        let http = match protocol {
            Protocol::Http => hyper::server::Server::http(server.host, server.port),
            Protocol::Https {cert, key} => hyper::server::Server::https(server.host, server.port, cert, key)
        };

        match threads {
            Some(threads) => http.listen_threads(server, threads),
            None => http.listen(server)
        }
    }

    ///Build a runnable instance of the server.
    pub fn build(self) -> (ServerInstance<R, C>, Protocol) {
        (ServerInstance {
            handlers: self.handlers,
            port: self.port,
            host: self.host,
            server: self.server,
            content_type: self.content_type,
            cache: self.cache,
            cache_clean_interval: self.cache_clean_interval,
            last_cache_clean: RwLock::new(Timespec::new(0, 0)),
            context_plugins: self.context_plugins,
            response_plugins: self.response_plugins,
        },
        self.protocol)
    }
}

///A runnable instance of a server.
///
///It's not meant to be used directly,
///unless additional control is required.
///
///```no_run
///# use rustful::{Server, Handler, Context, Response};
///# struct R;
///# impl Handler for R {
///#     type Cache = ();
///#     fn handle_request(&self, _context: Context, _response: Response) {}
///# }
///# let router = R;
///let (server_instance, protocol) = Server::new().port(8080).handlers(router).build();
///```
pub struct ServerInstance<R, C> {
    handlers: R,

    port: Port,
    host: IpAddr,

    server: String,
    content_type: Mime,

    cache: C,
    cache_clean_interval: Option<i64>,
    last_cache_clean: RwLock<Timespec>,

    context_plugins: Vec<Box<ContextPlugin<Cache=C> + Send + Sync>>,
    response_plugins: Vec<Box<ResponsePlugin + Send + Sync>>
}

impl<R, C: Cache> ServerInstance<R, C> {

    fn modify_context(&self, context: &mut Context<C>) -> ContextAction {
        let mut result = ContextAction::Continue;

        for plugin in &self.context_plugins {
            result = match result {
                ContextAction::Continue => plugin.modify(context),
                _ => return result
            };
        }

        result
    }

}

impl<R, H, C> HyperHandler for ServerInstance<R, C>
    where
    R: Router<Handler=H> + Send + Sync,
    H: Handler<Cache=C> + Send + Sync,
    C: Cache + Send + Sync,
{
    fn handle(&self, mut request: hyper::server::request::Request, writer: hyper::server::response::Response) {
        let request_uri = request.uri.clone();
        let request_method = request.method.clone();
        let mut request_headers = Headers::new();
        std::mem::swap(&mut request_headers, &mut request.headers);

        let mut response = Response::new(writer, &self.response_plugins);
        response.set_header(Date(time::now_utc()));
        response.set_header(ContentType(self.content_type.clone()));
        response.set_header(hyper::header::Server(self.server.clone()));

        let path_components = match request_uri {
            RequestUri::AbsoluteUri(url) => {
                Some((
                    url.serialize_path().map(|p| p.into_bytes()).unwrap_or_else(|| vec!['/' as u8]),
                    url.query_pairs().unwrap_or_else(|| Vec::new()).into_iter().collect(),
                    url.fragment
                ))
            },
            RequestUri::AbsolutePath(path) => {
                let (path, query, fragment) = parse_path(path);
                Some((path.into_bytes(), query, fragment))
            },
            _ => None //TODO: Handle *
        };

        match path_components {
            Some((path, query, fragment)) => {

                let mut context = Context {
                    headers: request_headers,
                    method: request_method,
                    path: lossy_utf8_percent_decode(&path),
                    variables: HashMap::new(),
                    query: query,
                    fragment: fragment,
                    cache: &self.cache,
                    body_reader: context::BodyReader::from_request(request)
                };

                match self.modify_context(&mut context) {
                    ContextAction::Continue => {
                        match self.handlers.find(&context.method, &context.path) {
                            Some((handler, variables)) => {
                                context.variables = variables;
                                handler.handle_request(context, response);
                            },
                            None => {
                                response.set_header(ContentLength(0));
                                response.set_status(StatusCode::NotFound);
                            }
                        }
                    },
                    ContextAction::Abort(status) => {
                        response.set_header(ContentLength(0));
                        response.set_status(status);
                    }
                }
            },
            None => {
                response.set_header(ContentLength(0));
                response.set_status(StatusCode::BadRequest);
            }
        }

        self.cache_clean_interval.map(|t| {
            let clean_time = {
                let last_cache_clean = self.last_cache_clean.read().unwrap();
                Timespec::new(last_cache_clean.sec + t, last_cache_clean.nsec)
            };

            if time::get_time() > clean_time {
                *self.last_cache_clean.write().unwrap() = time::get_time();
                self.cache.free_unused();
            }
        });
    }
}

fn parse_path(path: String) -> (String, HashMap<String, String>, Option<String>) {
    match path.find('?') {
        Some(index) => {
            let (query, fragment) = parse_fragment(&path[index+1..]);
            (path[..index].to_owned(), utils::parse_parameters(query.as_bytes()), fragment.map(|f| f.to_owned()))
        },
        None => {
            let (path, fragment) = parse_fragment(&path);
            (path.to_owned(), HashMap::new(), fragment.map(|f| f.to_owned()))
        }
    }
}

fn parse_fragment<'a>(path: &'a str) -> (&'a str, Option<&'a str>) {
    match path.find('#') {
        Some(index) => (&path[..index], Some(&path[index+1..])),
        None => (path, None)
    }
}




#[test]
fn parse_path_parts() {
    let with = "this".to_owned();
    let and = "that".to_owned();
    let (path, query, fragment) = parse_path(String::from_str("/path/to/something?with=this&and=that#lol"));
    assert_eq!(path, String::from_str("/path/to/something"));
    assert_eq!(query.get("with"), Some(&with));
    assert_eq!(query.get("and"), Some(&and));
    assert_eq!(fragment, Some(String::from_str("lol")));
}

#[test]
fn parse_strange_path() {
    let with = "this".to_owned();
    let and = "what?".to_owned();
    let (path, query, fragment) = parse_path(String::from_str("/path/to/something?with=this&and=what?#"));
    assert_eq!(path, String::from_str("/path/to/something"));
    assert_eq!(query.get("with"), Some(&with));
    assert_eq!(query.get("and"), Some(&and));
    assert_eq!(fragment, Some(String::from_str("")));
}

#[test]
fn parse_missing_path_parts() {
    let with = "this".to_owned();
    let and = "that".to_owned();
    let (path, query, fragment) = parse_path(String::from_str("/path/to/something?with=this&and=that"));
    assert_eq!(path, String::from_str("/path/to/something"));
    assert_eq!(query.get("with"), Some(&with));
    assert_eq!(query.get("and"), Some(&and));
    assert_eq!(fragment, None);


    let (path, query, fragment) = parse_path(String::from_str("/path/to/something#lol"));
    assert_eq!(path, String::from_str("/path/to/something"));
    assert_eq!(query.len(), 0);
    assert_eq!(fragment, Some(String::from_str("lol")));


    let (path, query, fragment) = parse_path(String::from_str("?with=this&and=that#lol"));
    assert_eq!(path, String::from_str(""));
    assert_eq!(query.get("with"), Some(&with));
    assert_eq!(query.get("and"), Some(&and));
    assert_eq!(fragment, Some(String::from_str("lol")));
}