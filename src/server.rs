//!Server configuration and instance.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::borrow::ToOwned;

use time;

use url::percent_encoding::{percent_decode, percent_decode_to};
use url::{Url, SchemeData};

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

use context::{self, Context, Uri};
use context::hypermedia::Hypermedia;
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
use Parameters;
use MaybeUtf8Owned;

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

    ///Start the server.
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

struct ParsedUri {
    host: Option<(String, Option<u16>)>,
    uri: Uri,
    query: Parameters,
    fragment: Option<MaybeUtf8Owned>
}

impl<R: Router> HyperHandler for ServerInstance<R> {
    fn handle(&self, request: hyper::server::request::Request, writer: hyper::server::response::Response) {
        let (
            request_addr,
            request_method,
            mut request_headers,
            request_uri,
            request_version,
            request_reader
        ) = request.deconstruct();

        let mut response = Response::new(writer, &self.response_filters, &*self.log, &self.global);
        response.headers_mut().set(Date(HttpDate(time::now_utc())));
        response.headers_mut().set(ContentType(self.content_type.clone()));
        response.headers_mut().set(hyper::header::Server(self.server.clone()));

        let path_components = match request_uri {
            RequestUri::AbsoluteUri(url) => Some(parse_url(url)),
            RequestUri::AbsolutePath(path) => Some(parse_path(&path)),
            RequestUri::Star => {
                Some(ParsedUri {
                    host: None,
                    uri: Uri::Asterisk,
                    query: Parameters::new(),
                    fragment: None
                })
            },
            _ => None
        };

        match path_components {
            Some(ParsedUri{ host, uri, query, fragment }) => {
                if let Some((name, port)) = host {
                    request_headers.set(::header::Host {
                        hostname: name,
                        port: port
                    });
                }

                let body = context::body::BodyReader::from_reader(request_reader, &request_headers);

                let mut context = Context {
                    headers: request_headers,
                    http_version: request_version,
                    method: request_method,
                    address: request_addr,
                    uri: uri,
                    hypermedia: Hypermedia::new(),
                    variables: Parameters::new(),
                    query: query.into(),
                    fragment: fragment,
                    log: &*self.log,
                    global: &self.global,
                    body: body
                };

                let mut filter_storage = AnyMap::new();

                match self.modify_context(&mut filter_storage, &mut context) {
                    ContextAction::Next => {
                        *response.filter_storage_mut() = filter_storage;

                        let endpoint = context.uri.as_path().map(|path| self.handlers.find(&context.method, path)).unwrap_or_else(|| {
                            Endpoint {
                                handler: None,
                                variables: HashMap::new(),
                                hypermedia: Hypermedia::new()
                            }
                        });

                        let Endpoint {
                            handler,
                            variables,
                            hypermedia
                        } = endpoint;

                        if let Some(handler) = handler.or(self.fallback_handler.as_ref()) {
                            context.hypermedia = hypermedia;
                            context.variables = variables.into();
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

fn parse_path(path: &str) -> ParsedUri {
    match path.find('?') {
        Some(index) => {
            let (query, fragment) = parse_fragment(&path[index+1..]);

            let mut path = percent_decode(path[..index].as_bytes());
            if path.is_empty() {
                path.push('/' as u8);
            }

            ParsedUri {
                host: None,
                uri: Uri::Path(path),
                query: utils::parse_parameters(query.as_bytes()),
                fragment: fragment.map(|f| percent_decode(f.as_bytes()).into())
            }
        },
        None => {
            let (path, fragment) = parse_fragment(&path);

            let mut path = percent_decode(path.as_bytes());
            if path.is_empty() {
                path.push('/' as u8);
            }

            ParsedUri {
                host: None,
                uri: Uri::Path(path),
                query: Parameters::new(),
                fragment: fragment.map(|f| percent_decode(f.as_bytes()).into())
            }
        }
    }
}

fn parse_fragment<'a>(path: &'a str) -> (&'a str, Option<&'a str>) {
    match path.find('#') {
        Some(index) => (&path[..index], Some(&path[index+1..])),
        None => (path, None)
    }
}

fn parse_url(url: Url) -> ParsedUri {
    let mut path = Vec::new();
    for component in url.path().unwrap_or(&[]) {
        path.push('/' as u8);
        percent_decode_to(component.as_bytes(), &mut path);
    }
    if path.is_empty() {
        path.push('/' as u8);
    }

    let query = url.query_pairs()
            .unwrap_or_else(|| Vec::new())
            .into_iter()
            .collect();

    let host = if let SchemeData::Relative(data) = url.scheme_data {
        Some((data.host.serialize(), data.port))
    } else {
        None
    };

    ParsedUri {
        host: host,
        uri: Uri::Path(path),
        query: query,
        fragment: url.fragment.map(|f| percent_decode(f.as_bytes()).into())
    }
}


#[test]
fn parse_path_parts() {
    let with = "this".to_owned().into();
    let and = "that".to_owned().into();
    let ParsedUri { uri, query, fragment, .. } = parse_path("/path/to/something?with=this&and=that#lol");
    assert_eq!(uri.as_path(), Some("/path/to/something".as_ref()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, Some("lol".to_owned().into()));
}

#[test]
fn parse_strange_path() {
    let with = "this".to_owned().into();
    let and = "what?".to_owned().into();
    let ParsedUri { uri, query, fragment, .. } = parse_path("/path/to/something?with=this&and=what?#");
    assert_eq!(uri.as_path(), Some("/path/to/something".as_ref()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, Some(String::new().into()));
}

#[test]
fn parse_missing_path_parts() {
    let with = "this".to_owned().into();
    let and = "that".to_owned().into();
    let ParsedUri { uri, query, fragment, .. } = parse_path("/path/to/something?with=this&and=that");
    assert_eq!(uri.as_path(), Some("/path/to/something".as_ref()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, None);


    let ParsedUri { uri, query, fragment, .. } = parse_path("/path/to/something#lol");
    assert_eq!(uri.as_path(), Some("/path/to/something".as_ref()));
    assert_eq!(query.len(), 0);
    assert_eq!(fragment, Some("lol".to_owned().into()));


    let ParsedUri { uri, query, fragment, .. } = parse_path("?with=this&and=that#lol");
    assert_eq!(uri.as_path(), Some("/".as_ref()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, Some("lol".to_owned().into()));
}


#[test]
fn parse_url_parts() {
    let with = "this".to_owned().into();
    let and = "that".to_owned().into();
    let url = Url::parse("http://example.com/path/to/something?with=this&and=that#lol").unwrap();
    let ParsedUri { uri, query, fragment, .. } = parse_url(url);
    assert_eq!(uri.as_path(), Some("/path/to/something".as_ref()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, Some("lol".to_owned().into()));
}

#[test]
fn parse_strange_url() {
    let with = "this".to_owned().into();
    let and = "what?".to_owned().into();
    let url = Url::parse("http://example.com/path/to/something?with=this&and=what?#").unwrap();
    let ParsedUri { uri, query, fragment, .. } = parse_url(url);
    assert_eq!(uri.as_path(), Some("/path/to/something".as_ref()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, Some(String::new().into()));
}

#[test]
fn parse_missing_url_parts() {
    let with = "this".to_owned().into();
    let and = "that".to_owned().into();
    let url = Url::parse("http://example.com/path/to/something?with=this&and=that").unwrap();
    let ParsedUri { uri, query, fragment, .. } = parse_url(url);
    assert_eq!(uri.as_path(), Some("/path/to/something".as_ref()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, None);


    let url = Url::parse("http://example.com/path/to/something#lol").unwrap();
    let ParsedUri { uri, query, fragment, .. } = parse_url(url);
    assert_eq!(uri.as_path(), Some("/path/to/something".as_ref()));
    assert_eq!(query.len(), 0);
    assert_eq!(fragment, Some("lol".to_owned().into()));


    let url = Url::parse("http://example.com?with=this&and=that#lol").unwrap();
    let ParsedUri { uri, query, fragment, .. } = parse_url(url);
    assert_eq!(uri.as_path(), Some("/".as_ref()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, Some("lol".to_owned().into()));
}