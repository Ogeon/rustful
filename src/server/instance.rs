use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};

use time;

use num_cpus;

use url::percent_encoding::percent_decode;
use url::Url;

use hyper;
use hyper::server::Handler as HyperHandler;
use hyper::header::{Date, ContentType};
use hyper::mime::Mime;
use hyper::uri::RequestUri;

pub use hyper::server::Listening;

use anymap::AnyMap;

use StatusCode;

use context::{self, Context, UriPath, MaybeUtf8Owned, Parameters};
use filter::{FilterContext, ContextFilter, ContextAction, ResponseFilter};
use handler::{HandleRequest, Environment};
use response::Response;
use header::HttpDate;
use server::{Global, KeepAlive};
use net::SslServer;

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
///#     fn handle(&self, _context: Context, _response: Response) {}
///# }
///# let router = R;
///let server_instance = Server {
///    host: 8080.into(),
///    handlers: router,
///    ..Server::default()
///}.build();
///```
pub struct ServerInstance<R> {
    handlers: R,

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

impl<R: HandleRequest + 'static> ServerInstance<R> {
    ///Create a new server instance, with the provided configuration. This is
    ///the same as `Server{...}.build()`.
    pub fn new(config: Server<R>) -> ServerInstance<R> {
        ServerInstance {
            handlers: config.handlers,
            host: config.host.into(),
            server: config.server,
            content_type: config.content_type,
            threads: config.threads.unwrap_or_else(|| (num_cpus::get() * 5) / 4),
            keep_alive: config.keep_alive,
            threads_in_use: AtomicUsize::new(0),
            context_filters: config.context_filters,
            response_filters: config.response_filters,
            global: config.global,
        }
    }

    ///Start the server.
    pub fn run(self) -> HttpResult<Listening> {
        let host = self.host;
        let threads = self.threads;
        let mut server = hyper::server::Server::http(host)?;
        server.keep_alive(self.keep_alive.as_ref().map(|k| k.timeout));
        server.handle_threads(self, threads)
    }

    ///Start the server with SSL.
    pub fn run_https<S: SslServer + Clone + Send + 'static>(self, ssl: S) -> HttpResult<Listening> {
        let host = self.host;
        let threads = self.threads;
        let mut server = hyper::server::Server::https(host, ssl)?;
        server.keep_alive(self.keep_alive.as_ref().map(|k| k.timeout));
        server.handle_threads(self, threads)
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
    uri_path: UriPath,
    query: Parameters,
    fragment: Option<MaybeUtf8Owned>
}

impl<R: HandleRequest + 'static> HyperHandler for ServerInstance<R> {
    fn handle<'a, 'b>(&'a self, request: hyper::server::request::Request<'a, 'b>, writer: hyper::server::response::Response<'a>) {
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
                    uri_path: UriPath::Asterisk,
                    query: Parameters::new(),
                    fragment: None
                })
            },
            _ => None
        };

        match path_components {
            Some(ParsedUri{ host, uri_path, query, fragment }) => {
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
                    uri_path: uri_path,
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

                        let path = context.uri_path.clone();
                        if let Some(path) = path.as_path() {
                            let result = self.handlers.handle_request(Environment {
                                context: context,
                                response: response,
                                route_state: (&path[..]).into(),
                            });

                            if let Err(mut environment) = result {
                                if environment.response.status() == StatusCode::Ok {
                                    environment.response.set_status(StatusCode::NotFound);
                                }
                            }
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
                uri_path: UriPath::Path(path.into()),
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
                uri_path: UriPath::Path(path.into()),
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
        uri_path: UriPath::Path(path.into()),
        query: query,
        fragment: url.fragment().as_ref().map(|f| percent_decode(f.as_bytes()).collect::<Vec<_>>().into())
    }
}


#[test]
fn parse_path_parts() {
    let with = "this".to_owned().into();
    let and = "that".to_owned().into();
    let ParsedUri { uri_path, query, fragment, .. } = parse_path("/path/to/something?with=this&and=that#lol");
    assert_eq!(uri_path.as_path(), Some("/path/to/something".into()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, Some("lol".to_owned().into()));
}

#[test]
fn parse_strange_path() {
    let with = "this".to_owned().into();
    let and = "what?".to_owned().into();
    let ParsedUri { uri_path, query, fragment, .. } = parse_path("/path/to/something?with=this&and=what?#");
    assert_eq!(uri_path.as_path(), Some("/path/to/something".into()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, Some(String::new().into()));
}

#[test]
fn parse_missing_path_parts() {
    let with = "this".to_owned().into();
    let and = "that".to_owned().into();
    let ParsedUri { uri_path, query, fragment, .. } = parse_path("/path/to/something?with=this&and=that");
    assert_eq!(uri_path.as_path(), Some("/path/to/something".into()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, None);


    let ParsedUri { uri_path, query, fragment, .. } = parse_path("/path/to/something#lol");
    assert_eq!(uri_path.as_path(), Some("/path/to/something".into()));
    assert_eq!(query.len(), 0);
    assert_eq!(fragment, Some("lol".to_owned().into()));


    let ParsedUri { uri_path, query, fragment, .. } = parse_path("?with=this&and=that#lol");
    assert_eq!(uri_path.as_path(), Some("/".into()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, Some("lol".to_owned().into()));
}


#[test]
fn parse_url_parts() {
    let with = "this".to_owned().into();
    let and = "that".to_owned().into();
    let url = Url::parse("http://example.com/path/to/something?with=this&and=that#lol").unwrap();
    let ParsedUri { uri_path, query, fragment, .. } = parse_url(&url);
    assert_eq!(uri_path.as_path(), Some("/path/to/something".into()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, Some("lol".to_owned().into()));
}

#[test]
fn parse_strange_url() {
    let with = "this".to_owned().into();
    let and = "what?".to_owned().into();
    let url = Url::parse("http://example.com/path/to/something?with=this&and=what?#").unwrap();
    let ParsedUri { uri_path, query, fragment, .. } = parse_url(&url);
    assert_eq!(uri_path.as_path(), Some("/path/to/something".into()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, Some(String::new().into()));
}

#[test]
fn parse_missing_url_parts() {
    let with = "this".to_owned().into();
    let and = "that".to_owned().into();
    let url = Url::parse("http://example.com/path/to/something?with=this&and=that").unwrap();
    let ParsedUri { uri_path, query, fragment, .. } = parse_url(&url);
    assert_eq!(uri_path.as_path(), Some("/path/to/something".into()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, None);


    let url = Url::parse("http://example.com/path/to/something#lol").unwrap();
    let ParsedUri { uri_path, query, fragment, .. } = parse_url(&url);
    assert_eq!(uri_path.as_path(), Some("/path/to/something".into()));
    assert_eq!(query.len(), 0);
    assert_eq!(fragment, Some("lol".to_owned().into()));


    let url = Url::parse("http://example.com?with=this&and=that#lol").unwrap();
    let ParsedUri { uri_path, query, fragment, .. } = parse_url(&url);
    assert_eq!(uri_path.as_path(), Some("/".into()));
    assert_eq!(query.get_raw("with"), Some(&with));
    assert_eq!(query.get_raw("and"), Some(&and));
    assert_eq!(fragment, Some("lol".to_owned().into()));
}
