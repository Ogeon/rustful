//!Server configuration and instance.

#![stable]

use std::collections::HashMap;
use std::net::{SocketAddr, SocketAddrV4, SocketAddrV6, Ipv4Addr};
use std::borrow::ToOwned;
use std::path::Path;

use time;

use url::percent_encoding::lossy_utf8_percent_decode;

use hyper;
use hyper::server::Handler as HyperHandler;
use hyper::header::{Date, ContentType, ContentLength};
use hyper::mime::Mime;
use hyper::uri::RequestUri;

pub use hyper::server::Listening;

use anymap::AnyMap;

use StatusCode;

use context::{self, Context};
use plugin::{PluginContext, ContextPlugin, ContextAction, ResponsePlugin};
use router::Router;
use handler::Handler;
use response::Response;
use log::{Log, StdOut};
use header::HttpDate;

use Scheme;

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
pub struct Server<'s, R> {
    ///One or several response handlers.
    pub handlers: R,

    ///The host address and port where the server will listen for requests.
    pub host: SocketAddr,

    ///The type of HTTP protocol.
    pub protocol: Scheme<'s>,

    ///The number of threads to be used in the server task pool.
    ///The default (`None`) is based on recommendations from the system.
    pub threads: Option<usize>,

    ///The content of the server header.
    pub server: String,

    ///The default media type.
    pub content_type: Mime,

    ///Tool for printing to a log.
    pub log: Box<Log + Send + Sync>,

    ///The context plugin stack.
    #[unstable]
    pub context_plugins: Vec<Box<ContextPlugin + Send + Sync>>,

    ///The response plugin stack.
    #[unstable]
    pub response_plugins: Vec<Box<ResponsePlugin + Send + Sync>>
}

impl<'s> Server<'s, ()> {
    ///Create a new empty server which will listen on host address `0.0.0.0` and port `80`.
    pub fn new() -> Server<'s, ()> {
        Server {
            handlers: (),
            host: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 80)),
            protocol: Scheme::Http,
            threads: None,
            server: "rustful".to_owned(),
            content_type: Mime(
                hyper::mime::TopLevel::Text,
                hyper::mime::SubLevel::Plain,
                vec![(hyper::mime::Attr::Charset, hyper::mime::Value::Utf8)]
            ),
            log: Box::new(StdOut),
            context_plugins: Vec::new(),
            response_plugins: Vec::new()
        }
    }
}

impl<'s, R> Server<'s, R> {
    ///Set request handlers.
    pub fn handlers<NewRouter: Router<Handler=H>, H: Handler>(self, handlers: NewRouter) -> Server<'s, NewRouter> {
        Server {
            handlers: handlers,
            host: self.host,
            protocol: self.protocol,
            threads: self.threads,
            server: self.server,
            content_type: self.content_type,
            log: self.log,
            context_plugins: self.context_plugins,
            response_plugins: self.response_plugins,
        }
    }

    ///Change the port. Default is `80`.
    pub fn port(mut self, port: u16) -> Server<'s, R> {
        self.host = match self.host {
            SocketAddr::V4(addr) => SocketAddr::V4(SocketAddrV4::new(addr.ip().clone(), port)),
            SocketAddr::V6(addr) => {
                SocketAddr::V6(SocketAddrV6::new(addr.ip().clone(), port, addr.flowinfo(), addr.scope_id()))
            }
        };
        self
    }

    ///Change the host address. Default is `0.0.0.0:80`.
    pub fn host(mut self, host: SocketAddr) -> Server<'s, R> {
        self.host = host;
        self
    }

    ///Change the protocol to HTTPS.
    pub fn https(mut self, cert: &'s Path, key: &'s Path) -> Server<'s, R> {
        self.protocol = Scheme::Https {
            cert: cert,
            key: key
        };
        self
    }

    ///Set the number of threads to be used in the server task pool.
    ///
    ///Passing `None` will set it to the default number of threads,
    ///based on recommendations from the system.
    pub fn threads(mut self, threads: Option<usize>) -> Server<'s, R> {
        self.threads = threads;
        self
    }

    ///Change the server response header. Default is `rustful`.
    pub fn server_name(mut self, name: String) -> Server<'s, R> {
        self.server = name;
        self
    }

    ///Change the default content type. Default is `text/plain`.
    pub fn content_type(mut self, content_type: Mime) -> Server<'s, R> {
        self.content_type = content_type;
        self
    }

    ///Change log tool. Default is to print to standard output.
    pub fn log<L: Log + Send + Sync + 'static>(mut self, log: L) -> Server<'s, R> {
        self.log = Box::new(log);
        self
    }

    ///Add a context plugin to the plugin stack.
    #[unstable]
    pub fn with_context_plugin<P: ContextPlugin + Send + Sync + 'static>(mut self, plugin: P) ->  Server<'s, R> {
        self.context_plugins.push(Box::new(plugin));
        self
    }

    ///Add a response plugin to the plugin stack.
    #[unstable]
    pub fn with_response_plugin<P: ResponsePlugin + Send + Sync + 'static>(mut self, plugin: P) ->  Server<'s, R> {
        self.response_plugins.push(Box::new(plugin));
        self
    }
}

impl<'s, R, H> Server<'s, R>
    where
    R: Router<Handler=H> + Send + Sync + 'static,
    H: Handler + Send + Sync + 'static
{
    ///Start the server.
    pub fn run(self) -> hyper::HttpResult<Listening> {
        let threads = self.threads;
        let (server, protocol) = self.build();
        let host = server.host;
        let http = match protocol {
            Scheme::Http => hyper::server::Server::http(server),
            Scheme::Https {cert, key} => hyper::server::Server::https(server, cert, key)
        };

        match threads {
            Some(threads) => http.listen_threads(host, threads),
            None => http.listen(host)
        }
    }

    ///Build a runnable instance of the server.
    pub fn build(self) -> (ServerInstance<R>, Scheme<'s>) {
        (ServerInstance {
            handlers: self.handlers,
            host: self.host,
            server: self.server,
            content_type: self.content_type,
            log: self.log,
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
///#     fn handle_request(&self, _context: Context, _response: Response) {}
///# }
///# let router = R;
///let (server_instance, protocol) = Server::new().port(8080).handlers(router).build();
///```
pub struct ServerInstance<R> {
    handlers: R,

    host: SocketAddr,

    server: String,
    content_type: Mime,

    log: Box<Log + Send + Sync>,

    context_plugins: Vec<Box<ContextPlugin + Send + Sync>>,
    response_plugins: Vec<Box<ResponsePlugin + Send + Sync>>
}

impl<R> ServerInstance<R> {

    fn modify_context(&self, plugin_storage: &mut AnyMap, context: &mut Context) -> ContextAction {
        let mut result = ContextAction::Continue;

        for plugin in &self.context_plugins {
            result = match result {
                ContextAction::Continue => {
                    let plugin_context = PluginContext {
                        storage: plugin_storage,
                        log: &*self.log
                    };
                    plugin.modify(plugin_context, context)
                },
                _ => return result
            };
        }

        result
    }

}

impl<R, H> HyperHandler for ServerInstance<R>
    where
    R: Router<Handler=H> + Send + Sync,
    H: Handler + Send + Sync
{
    fn handle(&self, request: hyper::server::request::Request, writer: hyper::server::response::Response) {
        let (
            request_addr,
            request_method,
            request_headers,
            request_uri,
            request_version,
            request_reader
        ) = request.deconstruct();

        let mut response = Response::new(writer, &self.response_plugins, &*self.log);
        response.set_header(Date(HttpDate(time::now_utc())));
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
                    http_version: request_version,
                    method: request_method,
                    address: request_addr,
                    path: lossy_utf8_percent_decode(&path),
                    variables: HashMap::new(),
                    query: query,
                    fragment: fragment,
                    log: &*self.log,
                    body_reader: context::BodyReader::from_reader(request_reader)
                };

                let mut plugin_storage = AnyMap::new();

                match self.modify_context(&mut plugin_storage, &mut context) {
                    ContextAction::Continue => {
                        *response.plugin_storage() = plugin_storage;
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
                        *response.plugin_storage() = plugin_storage;
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
    let (path, query, fragment) = parse_path("/path/to/something?with=this&and=that#lol".to_string());
    assert_eq!(path, "/path/to/something".to_string());
    assert_eq!(query.get("with"), Some(&with));
    assert_eq!(query.get("and"), Some(&and));
    assert_eq!(fragment, Some("lol".to_string()));
}

#[test]
fn parse_strange_path() {
    let with = "this".to_owned();
    let and = "what?".to_owned();
    let (path, query, fragment) = parse_path("/path/to/something?with=this&and=what?#".to_string());
    assert_eq!(path, "/path/to/something".to_string());
    assert_eq!(query.get("with"), Some(&with));
    assert_eq!(query.get("and"), Some(&and));
    assert_eq!(fragment, Some("".to_string()));
}

#[test]
fn parse_missing_path_parts() {
    let with = "this".to_owned();
    let and = "that".to_owned();
    let (path, query, fragment) = parse_path("/path/to/something?with=this&and=that".to_string());
    assert_eq!(path, "/path/to/something".to_string());
    assert_eq!(query.get("with"), Some(&with));
    assert_eq!(query.get("and"), Some(&and));
    assert_eq!(fragment, None);


    let (path, query, fragment) = parse_path("/path/to/something#lol".to_string());
    assert_eq!(path, "/path/to/something".to_string());
    assert_eq!(query.len(), 0);
    assert_eq!(fragment, Some("lol".to_string()));


    let (path, query, fragment) = parse_path("?with=this&and=that#lol".to_string());
    assert_eq!(path, "".to_string());
    assert_eq!(query.get("with"), Some(&with));
    assert_eq!(query.get("and"), Some(&and));
    assert_eq!(fragment, Some("lol".to_string()));
}