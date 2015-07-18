//!Server configuration and instance.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::borrow::ToOwned;

use time;

use url::percent_encoding::lossy_utf8_percent_decode;

use hyper;
use hyper::server::Handler as HyperHandler;
use hyper::header::{Date, ContentType};
use hyper::mime::Mime;
use hyper::uri::RequestUri;
#[cfg(feature = "ssl")]
use hyper::net::Openssl;

pub use hyper::server::Listening;

use anymap::AnyMap;

use StatusCode;

use context::{self, Context, Hypermedia};
use filter::{FilterContext, ContextFilter, ContextAction, ResponseFilter};
use router::{Router, Endpoint};
use handler::Handler;
use response::Response;
use log::{Log, StdOut};
use header::HttpDate;

use Scheme;
use Host;
use Global;
use HttpResult;

use utils;

///Used to set up and run a server.
///
///```no_run
///# use std::error::Error;
///# use rustful::{Server, Handler, Context, Response};
///# #[derive(Default)]
///# struct R;
///# impl Handler for R {
///#     fn handle_request(&self, _context: Context, _response: Response) {}
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
    ///(`None`) will cause the server to use a value based on recommendations
    ///from the system.
    pub threads: Option<usize>,

    ///The content of the server header. Default is `"rustful"`.
    pub server: String,

    ///The default media type. Default is `text/plain, charset: UTF-8`.
    pub content_type: Mime,

    ///Tool for printing to a log. The default is to print to standard output.
    pub log: Box<Log>,

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
            server: "rustful".to_owned(),
            content_type: Mime(
                hyper::mime::TopLevel::Text,
                hyper::mime::SubLevel::Plain,
                vec![(hyper::mime::Attr::Charset, hyper::mime::Value::Utf8)]
            ),
            log: Box::new(StdOut),
            global: Global::default(),
            context_filters: Vec::new(),
            response_filters: Vec::new(),
        }
    }

    ///Start the server.
    #[cfg(feature = "ssl")]
    pub fn run(self) -> HttpResult<Listening> {
        let threads = self.threads;
        let (server, scheme) = self.build();
        let host = server.host;
        match scheme {
            Scheme::Http => hyper::server::Server::http(host).and_then(|http| {
                if let Some(threads) = threads {
                    http.handle_threads(server, threads)
                } else {
                    http.handle(server)
                }
            }),
            Scheme::Https {cert, key} => {
                let ssl = try!(Openssl::with_cert_and_key(cert, key));
                hyper::server::Server::https(host, ssl).and_then(|https| {
                    if let Some(threads) = threads {
                        https.handle_threads(server, threads)
                    } else {
                        https.handle(server)
                    }
                })
            }
        }
    }

    #[cfg(not(feature = "ssl"))]
    pub fn run(self) -> HttpResult<Listening> {
        let threads = self.threads;
        let (server, _scheme) = self.build();
        let host = server.host;
        hyper::server::Server::http(host).and_then(|http| {
            if let Some(threads) = threads {
                http.handle_threads(server, threads)
            } else {
                http.handle(server)
            }
        })
    }

    ///Build a runnable instance of the server.
    pub fn build(self) -> (ServerInstance<R>, Scheme) {
        (ServerInstance {
            handlers: self.handlers,
            fallback_handler: self.fallback_handler,
            host: self.host.into(),
            server: self.server,
            content_type: self.content_type,
            log: self.log,
            context_filters: self.context_filters,
            response_filters: self.response_filters,
            global: self.global,
        },
        self.scheme)
    }
}

impl<R: Router + Default> Default for Server<R> {
    fn default() -> Server<R> {
        Server::new(R::default())
    }
}

///A runnable instance of a server.
///
///It's not meant to be used directly,
///unless additional control is required.
///
///```no_run
///# use rustful::{Server, Handler, Context, Response};
///# #[derive(Default)]
///# struct R;
///# impl Handler for R {
///#     fn handle_request(&self, _context: Context, _response: Response) {}
///# }
///# let router = R;
///let (server_instance, scheme) = Server {
///    host: 8080.into(),
///    handlers: router,
///    ..Server::default()
///}.build();
///```
pub struct ServerInstance<R: Router> {
    handlers: R,
    fallback_handler: Option<R::Handler>,

    host: SocketAddr,

    server: String,
    content_type: Mime,

    log: Box<Log>,

    context_filters: Vec<Box<ContextFilter>>,
    response_filters: Vec<Box<ResponseFilter>>,

    global: Global
}

impl<R: Router> ServerInstance<R> {

    fn modify_context(&self, filter_storage: &mut AnyMap, context: &mut Context) -> ContextAction {
        let mut result = ContextAction::Next;

        for filter in &self.context_filters {
            result = match result {
                ContextAction::Next => {
                    let filter_context = FilterContext {
                        storage: filter_storage,
                        log: &*self.log,
                        global: &self.global,
                    };
                    filter.modify(filter_context, context)
                },
                _ => return result
            };
        }

        result
    }

}

impl<R: Router> HyperHandler for ServerInstance<R> {
    fn handle(&self, request: hyper::server::request::Request, writer: hyper::server::response::Response) {
        let (
            request_addr,
            request_method,
            request_headers,
            request_uri,
            request_version,
            request_reader
        ) = request.deconstruct();

        let mut response = Response::new(writer, &self.response_filters, &*self.log, &self.global);
        response.headers_mut().set(Date(HttpDate(time::now_utc())));
        response.headers_mut().set(ContentType(self.content_type.clone()));
        response.headers_mut().set(hyper::header::Server(self.server.clone()));

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
                let body = {
                    let content_type = request_headers.get().map(|&ContentType(ref mime)| mime);
                    context::BodyReader::from_reader(request_reader, content_type)
                };
                let mut context = Context {
                    headers: request_headers,
                    http_version: request_version,
                    method: request_method,
                    address: request_addr,
                    path: lossy_utf8_percent_decode(&path),
                    hypermedia: Hypermedia::new(),
                    variables: HashMap::new(),
                    query: query,
                    fragment: fragment,
                    log: &*self.log,
                    global: &self.global,
                    body: body
                };

                let mut filter_storage = AnyMap::new();

                match self.modify_context(&mut filter_storage, &mut context) {
                    ContextAction::Next => {
                        *response.filter_storage_mut() = filter_storage;
                        let Endpoint {
                            handler,
                            variables,
                            hypermedia
                        } = self.handlers.find(&context.method, &context.path);
                        if let Some(handler) = handler.or(self.fallback_handler.as_ref()) {
                            context.hypermedia = hypermedia;
                            context.variables = variables;
                            handler.handle_request(context, response);
                        } else {
                            response.set_status(StatusCode::NotFound);
                        }
                    },
                    ContextAction::Abort(status) => {
                        *response.filter_storage_mut() = filter_storage;
                        response.set_status(status);
                    }
                }
            },
            None => {
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