use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use time;

use num_cpus;

use url::percent_encoding::percent_decode;
use url::Url;

use hyper::{self, Encoder, Decoder, Next, Control};
use hyper::server::Handler as HyperHandler;
use hyper::server::Response as HyperResponse;
use hyper::server::{HandlerFactory, Request};
use hyper::header::{Date, ContentType};
use hyper::mime::Mime;
use hyper::uri::RequestUri;
use hyper::net::{HttpListener, Transport};
#[cfg(feature = "ssl")]
use hyper::net::{Openssl, HttpsListener};

use anymap::Map;
use anymap::any::Any;

use StatusCode;

use context::{RawContext, UriPath, MaybeUtf8Owned, Parameters};
use filter::{FilterContext, ContextFilter, ContextAction, ResponseFilter};
use router::{Router, Endpoint};
use handler::{RawHandler, Factory};
use header::{Headers, HttpDate};
use server::{Scheme, Global, ThreadHandle, ThreadEnv};
use server::worker::{self, Worker};
use response::{RawResponse, ResponseHead};

use HttpResult;
use Server;

use utils;

struct Config<R: Router> {
    handlers: R,
    fallback_handler: Option<R::Handler>,

    server: String,
    content_type: Mime,

    context_filters: Vec<Box<ContextFilter>>,
}

///A running instance of a server.
///
///```
///#[macro_use] extern crate log;
///# extern crate rustful;
///# use std::error::Error;
///# use rustful::{Server, Handler, Context, Response};
///# #[derive(Default)]
///# struct R;
///# impl<'env> Handler<'env> for R {
///#     fn handle_request(&self, _context: Context, _response: Response) {}
///# }
///
///# fn main() {
///# let router = R;
///let server_result = Server {
///    host: 8080.into(),
///    handlers: router,
///    ..Server::default()
///}.run_and_then(|_, instance| {
///    //...do stuff...
///    //This is only necessary if it has to be closed before the program ends
///    instance.close();
///});
///
///if let Err(e) = server_result {
///    error!("could not start server: {}", e.description())
///}
///
/// //...do more stuff...
///
///# }
///```
pub struct ServerInstance<H: ThreadHandle> {
    ///The address where the server is listening.
    pub addr: SocketAddr,
    work_pool: Vec<H>,
    servers: Vec<(Option<::hyper::server::Listening>, H)>,
}

impl<H: ThreadHandle> ServerInstance<H> {
    ///Create and start a new server instance, with the provided
    ///configuration. This is the same as `Server{...}.run_in(...)`.
    pub fn run<'env, R, E>(config: Server<R>, env: E) -> HttpResult<ServerInstance<H>> where
        R: Router + 'env,
        R::Handler: Factory<'env>,
        E: ThreadEnv<'env, Handle=H>,
    {
        let Server {
            handlers,
            fallback_handler,
            server,
            content_type,
            context_filters,
            response_filters,
            global,
            host,
            threads,
            workers,
            timeout,
            max_sockets,
            keep_alive,
            scheme,
        } = config;

        let listener = try!(HttpListener::bind(&host.into()));
        let threads = threads.unwrap_or_else(|| (num_cpus::get() * 5) / 4);
        let workers = workers.unwrap_or_else(|| (num_cpus::get() * 5) / 4);
        let (pool_handles, worker) = worker::new(workers, &env);

        let factory = RequestHandlerFactory {
            config: Arc::new(Config {
                handlers: handlers,
                fallback_handler: fallback_handler,
                server: server,
                content_type: content_type,
                context_filters: context_filters,
            }),
            response_filters: Arc::new(response_filters),
            global: Arc::new(global),
            work_pool: worker,
        };

        let servers: Vec<_> = try!(
                ::std::iter::repeat(factory)
                    .take(threads)
                    .map(|factory| {
                        HyperServer::new(&scheme, try!(listener.try_clone()))
                            .and_then(|server| server.keep_alive(keep_alive)
                                .timeout(timeout)
                                .max_sockets(max_sockets)
                                .run(factory)
                            ).map(|(listening, server)| {
                                let handle = env.spawn(move || {
                                    server.run();
                                });
                                (Some(listening), handle)
                            })
                    }).collect()
            );

        Ok(ServerInstance {
            addr: try!(listener.0.local_addr()),
            work_pool: pool_handles,
            servers: servers,
        })
    }

    ///Shut down the server.
    pub fn close(mut self) {
        for &mut (ref mut server, _) in &mut self.servers {
            server.take().map(|s| s.close());
        }
    }
}

impl<H: ThreadHandle> Drop for ServerInstance<H> {
    fn drop(&mut self) {
        for (_, handle) in self.servers.drain(..) {
            handle.join();
        }
        for handle in self.work_pool.drain(..) {
            handle.join();
        }
    }
}

//Helper to handle multiple protocols.
enum HyperServer {
    Http(hyper::Server<HttpListener>),
    #[cfg(feature = "ssl")]
    Https(hyper::Server<HttpsListener<Openssl>>),
}

impl HyperServer {
    #[cfg(feature = "ssl")]
    fn new(scheme: &Scheme, listener: HttpListener) -> HttpResult<HyperServer> {
        match *scheme {
            Scheme::Http => Ok(HyperServer::Http(hyper::Server::new(listener))),
            Scheme::Https { ref cert, ref key } => {
                let ssl = try!(Openssl::with_cert_and_key(cert, key));
                let server = hyper::Server::new(HttpsListener::with_listener(listener.0, ssl));
                Ok(HyperServer::Https(server))
            },
        }
    }

    #[cfg(not(feature = "ssl"))]
    fn new(scheme: &Scheme, listener: HttpListener) -> HttpResult<HyperServer> {
        match *scheme {
            Scheme::Http => Ok(HyperServer::Http(hyper::Server::new(listener))),
        }
    }

    #[cfg(feature = "ssl")]
    fn keep_alive(self, enabled: bool) -> HyperServer {
        match self {
            HyperServer::Http(s) => HyperServer::Http(s.keep_alive(enabled)),
            HyperServer::Https(s) => HyperServer::Https(s.keep_alive(enabled)),
        }
    }

    #[cfg(not(feature = "ssl"))]
    fn keep_alive(self, enabled: bool) -> HyperServer {
        match self {
            HyperServer::Http(s) => HyperServer::Http(s.keep_alive(enabled)),
        }
    }

    #[cfg(feature = "ssl")]
    fn timeout(self, duration: Duration) -> HyperServer {
        match self {
            HyperServer::Http(s) => HyperServer::Http(s.idle_timeout(Some(duration))),
            HyperServer::Https(s) => HyperServer::Https(s.idle_timeout(Some(duration))),
        }
    }

    #[cfg(not(feature = "ssl"))]
    fn timeout(self, duration: Duration) -> HyperServer {
        match self {
            HyperServer::Http(s) => HyperServer::Http(s.idle_timeout(Some(duration))),
        }
    }

    #[cfg(feature = "ssl")]
    fn max_sockets(self, max: usize) -> HyperServer {
        match self {
            HyperServer::Http(s) => HyperServer::Http(s.max_sockets(max)),
            HyperServer::Https(s) => HyperServer::Https(s.max_sockets(max)),
        }
    }

    #[cfg(not(feature = "ssl"))]
    fn max_sockets(self, max: usize) -> HyperServer {
        match self {
            HyperServer::Http(s) => HyperServer::Http(s.max_sockets(max)),
        }
    }

    #[cfg(feature = "ssl")]
    fn run<'env, R: Router>(self, server: RequestHandlerFactory<'env, R>) -> HttpResult<(::hyper::server::Listening, ServerLoop<R>)> where
        R::Handler: Factory<'env>,
    {
        match self {
            HyperServer::Http(s) => s.handle(server).map(|(listening, server_loop)| (listening, ServerLoop::http(server_loop))),
            HyperServer::Https(s) => s.handle(server).map(|(listening, server_loop)| (listening, ServerLoop::https(server_loop))),
        }
    }

    #[cfg(not(feature = "ssl"))]
    fn run<'env, R: Router>(self, server: RequestHandlerFactory<'env, R>) -> HttpResult<(::hyper::server::Listening, ServerLoop<R>)> where
        R::Handler: Factory<'env>,
    {
        match self {
            HyperServer::Http(s) => s.handle(server).map(|(listening, server_loop)| (listening, ServerLoop::http(server_loop))),
        }
    }
}

#[must_use]
///A server event loop, waiting to be started.
struct ServerLoop<'env, R: Router>(ServerLoopKind<'env, R>) where R::Handler: Factory<'env>;

impl<'env, R: Router> ServerLoop<'env, R> where R::Handler: Factory<'env> {
    ///Run the server loop, and block this thread. This will otherwise happen
    ///automatically when it's dropped.
    pub fn run(self) {}

    fn http(server: hyper::server::ServerLoop<HttpListener, RequestHandlerFactory<'env, R>>) -> ServerLoop<R> {
        ServerLoop(ServerLoopKind::Http(server))
    }

    #[cfg(feature = "ssl")]
    fn https(server: hyper::server::ServerLoop<HttpsListener<Openssl>, RequestHandlerFactory<'env, R>>) -> ServerLoop<R> {
        ServerLoop(ServerLoopKind::Https(server))
    }
}

enum ServerLoopKind<'env, R: Router> where R::Handler: Factory<'env> {
    Http(hyper::server::ServerLoop<HttpListener, RequestHandlerFactory<'env, R>>),
    #[cfg(feature = "ssl")]
    Https(hyper::server::ServerLoop<HttpsListener<Openssl>, RequestHandlerFactory<'env, R>>),
}

struct RequestHandlerFactory<'env, R: Router> {
    config: Arc<Config<R>>,
    response_filters: Arc<Vec<Box<ResponseFilter>>>,
    global: Arc<Global>,
    work_pool: Worker<'env>,
}

impl<'env, R: Router> Clone for RequestHandlerFactory<'env, R> {
    fn clone(&self) -> RequestHandlerFactory<'env, R> {
        RequestHandlerFactory {
            config: self.config.clone(),
            response_filters: self.response_filters.clone(),
            global: self.global.clone(),
            work_pool: self.work_pool.clone(),
        }
    }
}

impl<'env, T: Transport, R: Router> HandlerFactory<T> for RequestHandlerFactory<'env, R> where
    for<'a, 'b> &'a mut Encoder<'b, T>: Into<::handler::RawEncoder<'a, 'b>>,
    for<'a, 'b> &'a mut Decoder<'b, T>: Into<::handler::RawDecoder<'a, 'b>>,
    R::Handler: Factory<'env>,
{
    type Output = RequestHandler<'env, R>;

    fn create(&mut self, control: Control) -> RequestHandler<'env, R> {
        RequestHandler::new(self.config.clone(), self.response_filters.clone(), self.global.clone(), control, self.work_pool.clone())
    }
}

struct ParsedUri {
    host: Option<(String, Option<u16>)>,
    uri_path: UriPath,
    query: Parameters,
    fragment: Option<MaybeUtf8Owned>
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

struct RequestHandler<'env, R: Router> where R::Handler: Factory<'env> {
    config: Arc<Config<R>>,
    global: Arc<Global>,
    response_filters: Arc<Vec<Box<ResponseFilter>>>,
    write_method: Option<WriteMethod<<R::Handler as ::handler::Factory<'env>>::Handler>>,

    control: Option<Control>,
    worker: Worker<'env>,
}

impl<'env, R: Router> RequestHandler<'env, R> where R::Handler: Factory<'env> {
    fn new(config: Arc<Config<R>>, response_filters: Arc<Vec<Box<ResponseFilter>>>, global: Arc<Global>, control: Control, worker: Worker<'env>) -> RequestHandler<'env, R> {
        RequestHandler {
            config: config,
            global: global,
            response_filters: response_filters,
            write_method: None,

            control: Some(control),
            worker: worker,
        }
    }
}

fn modify_context(context_filters: &[Box<ContextFilter>], global: &Global, filter_storage: &mut Map<Any + Send + 'static>, context: &mut RawContext) -> ContextAction {
    let mut result = ContextAction::Next;

    for filter in context_filters {
        result = match result {
            ContextAction::Next => {
                let filter_context = FilterContext {
                    storage: filter_storage,
                    global: global,
                };
                filter.modify(filter_context, context)
            },
            _ => return result
        };
    }

    result
}

impl<'env, T: Transport, R: Router> HyperHandler<T> for RequestHandler<'env, R> where
    for<'a, 'b> &'a mut Encoder<'b, T>: Into<::handler::RawEncoder<'a, 'b>>,
    for<'a, 'b> &'a mut Decoder<'b, T>: Into<::handler::RawDecoder<'a, 'b>>,
    R::Handler: Factory<'env>,
{
    fn on_request(&mut self, request: Request<T>) -> Next {
        if let Some(control) = self.control.take() {
            let mut response = RawResponse {
                status: StatusCode::Ok,
                headers: Headers::new(),
                filters: ::interface::response::make_response_filters(
                    self.response_filters.clone(),
                    self.global.clone(),
                ),
            };

            response.headers.set(Date(HttpDate(time::now_utc())));
            response.headers.set(ContentType(self.config.content_type.clone()));
            response.headers.set(::header::Server(self.config.server.clone()));

            let (method, request_uri, http_version, mut request_headers) = request.deconstruct();

            let path_components = match request_uri {
                RequestUri::AbsoluteUri(ref url) => Some(parse_url(url)),
                RequestUri::AbsolutePath(ref path) => Some(parse_path(path)),
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

            let (write_method, next) = match path_components {
                Some(ParsedUri{ host, uri_path, query, fragment }) => {
                    if request_headers.get::<::header::Host>().is_none() {
                        if let Some((name, port)) = host {
                            request_headers.set(::header::Host {
                                hostname: name,
                                port: port
                            });
                        }
                    }

                    let mut context = RawContext {
                        method: method,
                        http_version: http_version,
                        headers: request_headers,
                        uri_path: uri_path,
                        hyperlinks: vec![],
                        variables: Parameters::new(),
                        query: query,
                        fragment: fragment,
                        global: self.global.clone(),
                        control: control,
                        worker: self.worker.clone(),
                    };

                    let mut filter_storage = Map::new();

                    match modify_context(&self.config.context_filters, &self.global, &mut filter_storage, &mut context) {
                        ContextAction::Next => {
                            response.filters.storage = filter_storage;
                            let config = &self.config;

                            let endpoint = context.uri_path.as_path().map_or_else(|| {
                                Endpoint {
                                    handler: None,
                                    variables: HashMap::new(),
                                    hyperlinks: vec![]
                                }
                            }, |path| config.handlers.find(&context.method, &mut (&path[..]).into()));

                            let Endpoint {
                                handler,
                                variables,
                                hyperlinks
                            } = endpoint;

                            if let Some(handler) = handler.or(config.fallback_handler.as_ref()) {
                                context.hyperlinks = hyperlinks;
                                context.variables = variables.into();
                                let mut handler = handler.create(context, response);
                                let next = handler.on_request();
                                (WriteMethod::Handler(handler), next)
                            } else {
                                response.status = StatusCode::NotFound;
                                (
                                    WriteMethod::Error(Some(ResponseHead {
                                        status: response.status,
                                        headers: response.headers,
                                    })),
                                    Next::write()
                                )
                            }
                        },
                        ContextAction::Abort(status) => {
                            response.status = status;
                            (
                                WriteMethod::Error(Some(ResponseHead {
                                    status: response.status,
                                    headers: response.headers,
                                })),
                                Next::write()
                            )
                        }
                    }
                },
                None => {
                    response.status = StatusCode::BadRequest;
                    (
                        WriteMethod::Error(Some(ResponseHead {
                            status: response.status,
                            headers: response.headers,
                        })),
                        Next::write()
                    )
                }
            };

            self.write_method = Some(write_method);
            next
        } else {
            panic!("RequestHandler reused");
        }
    }

    fn on_request_readable(&mut self, decoder: &mut Decoder<T>) -> Next {
        if let Some(WriteMethod::Handler(ref mut handler)) = self.write_method {
            handler.on_request_readable(decoder.into())
        } else {
            Next::write()
        }
    }

    fn on_response(&mut self, response: &mut HyperResponse) -> Next {
        if let Some(ref mut method) = self.write_method {
            let (head, next) = match *method {
                WriteMethod::Handler(ref mut handler) => handler.on_response(),
                WriteMethod::Error(ref mut head) => (head.take().expect("missing response head"), Next::end()),
            };

            response.set_status(head.status);
            response.headers_mut().extend(head.headers.iter());

            next
        } else {
            panic!("missing write method")
        }
    }

    fn on_response_writable(&mut self, encoder: &mut Encoder<T>) -> Next {
        if let Some(WriteMethod::Handler(ref mut handler)) = self.write_method {
            handler.on_response_writable(encoder.into())
        } else {
            Next::end()
        }
    }
}

enum WriteMethod<H> {
    Handler(H),
    Error(Option<ResponseHead>),
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
