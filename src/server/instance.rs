use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
#[cfg(feature = "ssl")]
use std::path::PathBuf;

use time;

use num_cpus;

use url::percent_encoding::percent_decode;
use url::Url;

use hyper;
use hyper::server::Handler as HyperHandler;
use hyper::header::{Date, ContentType};
use hyper::mime::Mime;
use hyper::uri::RequestUri;
use hyper::net::HttpListener;
#[cfg(feature = "ssl")]
use hyper::net::{Openssl, HttpsListener};

pub use hyper::server::Listening;

use anymap::AnyMap;

use StatusCode;

use context::{self, Context, Uri, MaybeUtf8Owned, Parameters};
use filter::{FilterContext, ContextFilter, ContextAction, ResponseFilter};
use router::{Router, Endpoint};
use handler::Handler;
use response::Response;
use header::HttpDate;
use server::{Scheme, Global, KeepAlive};

use HttpResult;
use Server;

use utils;

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

    threads: usize,
    keep_alive: Option<KeepAlive>,
    threads_in_use: AtomicUsize,

    context_filters: Vec<Box<ContextFilter>>,
    response_filters: Vec<Box<ResponseFilter>>,

    global: Global,
}

impl<R: Router> ServerInstance<R> {
    ///Create a new server instance, with the provided configuration. This is
    ///the same as `Server{...}.build()`.
    pub fn new(config: Server<R>) -> (ServerInstance<R>, Scheme) {
        (ServerInstance {
            handlers: config.handlers,
            fallback_handler: config.fallback_handler,
            host: config.host.into(),
            server: config.server,
            content_type: config.content_type,
            threads: config.threads.unwrap_or_else(|| (num_cpus::get() * 5) / 4),
            keep_alive: config.keep_alive,
            threads_in_use: AtomicUsize::new(0),
            context_filters: config.context_filters,
            response_filters: config.response_filters,
            global: config.global,
        },
        config.scheme)
    }

    ///Start the server.
    #[cfg(feature = "ssl")]
    pub fn run(self, scheme: Scheme) -> HttpResult<Listening> {
        let host = self.host;
        let threads = self.threads;
        let mut server = match scheme {
            Scheme::Http => try!(HyperServer::http(host)),
            Scheme::Https {cert, key} => try!(HyperServer::https(host, cert, key)),
        };
        server.keep_alive(self.keep_alive.as_ref().map(|k| k.timeout));
        server.run(self, threads)
    }

    ///Start the server.
    #[cfg(not(feature = "ssl"))]
    pub fn run(self, _scheme: Scheme) -> HttpResult<Listening> {
        let host = self.host;
        let threads = self.threads;
        let mut server = try!(HyperServer::http(host));
        server.keep_alive(self.keep_alive.as_ref().map(|k| k.timeout));
        server.run(self, threads)
    }

    fn modify_context(&self, filter_storage: &mut AnyMap, context: &mut Context) -> ContextAction {
        let mut result = ContextAction::Next;

        for filter in &self.context_filters {
            result = match result {
                ContextAction::Next => {
                    let filter_context = FilterContext {
                        storage: filter_storage,
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

        let force_close = if let Some(ref keep_alive) = self.keep_alive {
            self.threads_in_use.load(Ordering::SeqCst) + keep_alive.free_threads > self.threads
        } else {
            false
        };

        let mut response = Response::new(writer, &self.response_filters, &self.global, force_close);
        response.headers_mut().set(Date(HttpDate(time::now_utc())));
        response.headers_mut().set(ContentType(self.content_type.clone()));
        response.headers_mut().set(hyper::header::Server(self.server.clone()));

        let path_components = match request_uri {
            RequestUri::AbsoluteUri(url) => Some(parse_url(&url)),
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
                    hyperlinks: vec![],
                    variables: Parameters::new(),
                    query: query.into(),
                    fragment: fragment,
                    global: &self.global,
                    body: body
                };

                let mut filter_storage = AnyMap::new();

                match self.modify_context(&mut filter_storage, &mut context) {
                    ContextAction::Next => {
                        *response.filter_storage_mut() = filter_storage;

                        let endpoint = context.uri.as_path().map_or_else(|| {
                            Endpoint {
                                handler: None,
                                variables: HashMap::new(),
                                hyperlinks: vec![]
                            }
                        }, |path| self.handlers.find(&context.method, &mut (&path[..]).into()));

                        let Endpoint {
                            handler,
                            variables,
                            hyperlinks
                        } = endpoint;

                        if let Some(handler) = handler.or(self.fallback_handler.as_ref()) {
                            context.hyperlinks = hyperlinks;
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

    fn on_connection_start(&self) {
        self.threads_in_use.fetch_add(1, Ordering::SeqCst);
    }

    fn on_connection_end(&self) {
        self.threads_in_use.fetch_sub(1, Ordering::SeqCst);
    }
}

fn parse_path(path: &str) -> ParsedUri {
    match path.find('?') {
        Some(index) => {
            let (query, fragment) = parse_fragment(&path[index+1..]);

            let mut path: Vec<_> = percent_decode(path[..index].as_bytes()).collect();
            if path.is_empty() {
                path.push(b'/');
            }

            ParsedUri {
                host: None,
                uri: Uri::Path(path.into()),
                query: utils::parse_parameters(query.as_bytes()),
                fragment: fragment.map(|f| percent_decode(f.as_bytes()).collect::<Vec<_>>().into()),
            }
        },
        None => {
            let (path, fragment) = parse_fragment(&path);

            let mut path: Vec<_> = percent_decode(path.as_bytes()).collect();
            if path.is_empty() {
                path.push(b'/');
            }

            ParsedUri {
                host: None,
                uri: Uri::Path(path.into()),
                query: Parameters::new(),
                fragment: fragment.map(|f| percent_decode(f.as_bytes()).collect::<Vec<_>>().into())
            }
        }
    }
}

fn parse_fragment(path: &str) -> (&str, Option<&str>) {
    match path.find('#') {
        Some(index) => (&path[..index], Some(&path[index+1..])),
        None => (path, None)
    }
}

fn parse_url(url: &Url) -> ParsedUri {
    let mut path = vec![];
    path.extend(percent_decode(url.path().as_bytes()));

    let query = url.query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    let host = url.host_str().map(|host| (host.into(), url.port()));

    ParsedUri {
        host: host,
        uri: Uri::Path(path.into()),
        query: query,
        fragment: url.fragment().as_ref().map(|f| percent_decode(f.as_bytes()).collect::<Vec<_>>().into())
    }
}

//Helper to handle multiple protocols.
enum HyperServer {
    Http(hyper::server::Server<HttpListener>),
    #[cfg(feature = "ssl")]
    Https(hyper::server::Server<HttpsListener<Openssl>>),
}

impl HyperServer {
    fn http(host: SocketAddr) -> HttpResult<HyperServer> {
        hyper::server::Server::http(host).map(HyperServer::Http)
    }

    #[cfg(feature = "ssl")]
    fn https(host: SocketAddr, cert: PathBuf, key: PathBuf) -> HttpResult<HyperServer> {
        let ssl = try!(Openssl::with_cert_and_key(cert, key));
        hyper::server::Server::https(host, ssl).map(HyperServer::Https)
    }

    #[cfg(feature = "ssl")]
    fn keep_alive(&mut self, timeout: Option<Duration>) {
        match *self {
            HyperServer::Http(ref mut s) => s.keep_alive(timeout),
            HyperServer::Https(ref mut s) => s.keep_alive(timeout),
        }
    }

    #[cfg(not(feature = "ssl"))]
    fn keep_alive(&mut self, timeout: Option<Duration>) {
        match *self {
            HyperServer::Http(ref mut s) => s.keep_alive(timeout),
        }
    }

    #[cfg(feature = "ssl")]
    fn run<R: Router>(self, server: ServerInstance<R>, threads: usize) -> HttpResult<Listening> {
        match self {
            HyperServer::Http(s) => s.handle_threads(server, threads),
            HyperServer::Https(s) => s.handle_threads(server, threads),
        }
    }

    #[cfg(not(feature = "ssl"))]
    fn run<R: Router>(self, server: ServerInstance<R>, threads: usize) -> HttpResult<Listening> {
        match self {
            HyperServer::Http(s) => s.handle_threads(server, threads),
        }
    }
}


#[test]
fn parse_path_parts() {
    let with = "this".to_owned().into();
    let and = "that".to_owned().into();
    let ParsedUri { uri, query, fragment, .. } = parse_path("/path/to/something?with=this&and=that#lol");
    assert_eq!(uri.as_path(), Some("/path/to/something".into()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, Some("lol".to_owned().into()));
}

#[test]
fn parse_strange_path() {
    let with = "this".to_owned().into();
    let and = "what?".to_owned().into();
    let ParsedUri { uri, query, fragment, .. } = parse_path("/path/to/something?with=this&and=what?#");
    assert_eq!(uri.as_path(), Some("/path/to/something".into()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, Some(String::new().into()));
}

#[test]
fn parse_missing_path_parts() {
    let with = "this".to_owned().into();
    let and = "that".to_owned().into();
    let ParsedUri { uri, query, fragment, .. } = parse_path("/path/to/something?with=this&and=that");
    assert_eq!(uri.as_path(), Some("/path/to/something".into()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, None);


    let ParsedUri { uri, query, fragment, .. } = parse_path("/path/to/something#lol");
    assert_eq!(uri.as_path(), Some("/path/to/something".into()));
    assert_eq!(query.len(), 0);
    assert_eq!(fragment, Some("lol".to_owned().into()));


    let ParsedUri { uri, query, fragment, .. } = parse_path("?with=this&and=that#lol");
    assert_eq!(uri.as_path(), Some("/".into()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, Some("lol".to_owned().into()));
}


#[test]
fn parse_url_parts() {
    let with = "this".to_owned().into();
    let and = "that".to_owned().into();
    let url = Url::parse("http://example.com/path/to/something?with=this&and=that#lol").unwrap();
    let ParsedUri { uri, query, fragment, .. } = parse_url(&url);
    assert_eq!(uri.as_path(), Some("/path/to/something".into()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, Some("lol".to_owned().into()));
}

#[test]
fn parse_strange_url() {
    let with = "this".to_owned().into();
    let and = "what?".to_owned().into();
    let url = Url::parse("http://example.com/path/to/something?with=this&and=what?#").unwrap();
    let ParsedUri { uri, query, fragment, .. } = parse_url(&url);
    assert_eq!(uri.as_path(), Some("/path/to/something".into()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, Some(String::new().into()));
}

#[test]
fn parse_missing_url_parts() {
    let with = "this".to_owned().into();
    let and = "that".to_owned().into();
    let url = Url::parse("http://example.com/path/to/something?with=this&and=that").unwrap();
    let ParsedUri { uri, query, fragment, .. } = parse_url(&url);
    assert_eq!(uri.as_path(), Some("/path/to/something".into()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, None);


    let url = Url::parse("http://example.com/path/to/something#lol").unwrap();
    let ParsedUri { uri, query, fragment, .. } = parse_url(&url);
    assert_eq!(uri.as_path(), Some("/path/to/something".into()));
    assert_eq!(query.len(), 0);
    assert_eq!(fragment, Some("lol".to_owned().into()));


    let url = Url::parse("http://example.com?with=this&and=that#lol").unwrap();
    let ParsedUri { uri, query, fragment, .. } = parse_url(&url);
    assert_eq!(uri.as_path(), Some("/".into()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, Some("lol".to_owned().into()));
}
