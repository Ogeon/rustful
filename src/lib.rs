#![crate_name = "rustful"]

#![comment = "RESTful web framework"]
#![license = "MIT"]
#![crate_type = "rlib"]

#![doc(html_root_url = "http://ogeon.github.io/rustful/doc/")]

#![feature(associated_types)]

#[cfg(test)]
extern crate test;

extern crate url;
extern crate time;
extern crate sync;
extern crate http;

pub use router::TreeRouter;

use http::server::Server as HttpServer;
use http::server::{ResponseWriter, Config};
use http::server::request::{AbsoluteUri, AbsolutePath};
use http::method::Method;
use http::status;
use http::status::{NotFound, BadRequest, Status};
use http::headers::content_type::MediaType;
use http::headers;

use std::io::{IoResult, IoError};
use std::io::net::ip::{SocketAddr, IpAddr, Ipv4Addr, Port};
use std::collections::HashMap;
use std::error::FromError;

use sync::{Arc, RWLock};

use time::Timespec;

use url::percent_encoding::lossy_utf8_percent_decode;

use self::ResponseAction::{Write, DoNothing, Error};

mod utils;

pub mod router;
pub mod cache;
pub mod request_extensions;


///The result from a `RequestPlugin`.
#[experimental]
pub enum RequestAction {
	///Continue to the next plugin in the stack.
	Continue(Request),

	///Abort and send HTTP status.
	Abort(status::Status)
}

///The result of a response action.
#[experimental]
pub enum ResponseError {
	///A response plugin failed.
    PluginError(String),

    ///There was an IO error.
    IoError(IoError)
}

impl FromError<IoError> for ResponseError {
	fn from_error(err: IoError) -> ResponseError {
		ResponseError::IoError(err)
	}
}

///The result from a `ResponsePlugin`.
#[experimental]
pub enum ResponseAction<'a> {
	///Continue to the next plugin and maybe write data.
	Write(Option<ResponseData<'a>>),

	///Do not continue to the next plugin.
	DoNothing,

	///Abort with an error.
	Error(String)
}

impl<'a> ResponseAction<'a> {
	pub fn write<'a, T: IntoResponseData<'a>>(data: Option<T>) -> ResponseAction<'a> {
		ResponseAction::Write(data.map(|d| d.into_response_data()))
	}

	pub fn do_nothing() -> ResponseAction<'static> {
		ResponseAction::DoNothing
	}

	pub fn error(message: String) -> ResponseAction<'static> {
		ResponseAction::Error(message)
	}
}

#[experimental]
pub enum ResponseData<'a> {
	///Data in byte form.
	Bytes(Vec<u8>),

	///Data in byte form.
	ByteSlice(&'a [u8]),

	///Data in string form.
	String(String),

	///Data in string form.
	StringSlice(&'a str)
}

impl<'a> ResponseData<'a> {
	///Borrow the content as a byte slice.
	pub fn as_bytes(&self) -> &[u8] {
		match self {
			&ResponseData::Bytes(ref bytes) => bytes.as_slice(),
			&ResponseData::ByteSlice(ref bytes) => *bytes,
			&ResponseData::String(ref string) => string.as_bytes(),
			&ResponseData::StringSlice(ref string) => string.as_bytes()
		}
	}

	///Turns the content into a byte vector. Slices are copied.
	pub fn into_bytes(self) -> Vec<u8> {
		match self {
			ResponseData::Bytes(bytes) => bytes,
			ResponseData::ByteSlice(bytes) => bytes.to_vec(),
			ResponseData::String(string) => string.into_bytes(),
			ResponseData::StringSlice(string) => string.as_bytes().to_vec()
		}
	}

	///Borrow the content as a string slice if the content is a string.
	///Returns an `None` if the content is a byte vector, a byte slice or if the action is `Error`.
	pub fn as_string(&self) -> Option<&str> {
		match self {
			&ResponseData::String(ref string) => Some(string.as_slice()),
			&ResponseData::StringSlice(ref string) => Some(string.as_slice()),
			_ => None
		}
	}

	///Extract the contained string or string slice if there is any.
	///Returns an `None` if the content is a byte vector, a byte slice or if the action is `Error`.
	///Slices are copied.
	pub fn into_string(self) -> Option<String> {
		match self {
			ResponseData::String(string) => Some(string),
			ResponseData::StringSlice(string) => Some(string.into_string()),
			_ => None
		}
	}
}



pub trait IntoResponseData<'a> {
	fn into_response_data(self) -> ResponseData<'a>;
}

impl IntoResponseData<'static> for Vec<u8> {
	fn into_response_data(self) -> ResponseData<'static> {
		ResponseData::Bytes(self)
	}
}

impl<'a> IntoResponseData<'a> for &'a [u8] {
	fn into_response_data(self) -> ResponseData<'a> {
		ResponseData::ByteSlice(self)
	}
}

impl IntoResponseData<'static> for String {
	fn into_response_data(self) -> ResponseData<'static> {
		ResponseData::String(self)
	}
}

impl<'a> IntoResponseData<'a> for &'a str {
	fn into_response_data(self) -> ResponseData<'a> {
		ResponseData::StringSlice(self)
	}
}

impl<'a> IntoResponseData<'a> for ResponseData<'a> {
	fn into_response_data(self) -> ResponseData<'a> {
		self
	}
}


///Routers are used to store request handlers.
pub trait Router<Handler> {
	///Find and return the matching handler and variable values.
	fn find(&self, method: &Method, path: &str) -> Option<(&Handler, HashMap<String, String>)>;
}


///A trait for request handlers.
pub trait Handler<C> {
	fn handle_request(&self, request: Request, cache: &C, response: &mut Response);
}

impl<C> Handler<C> for () {
	fn handle_request(&self, _request: Request, _cache: &C, _response: &mut Response) {}
}

impl<C> Handler<C> for fn(Request, &mut Response) {
	fn handle_request(&self, request: Request, _cache: &C, response: &mut Response) {
		(*self)(request, response);
	}
}

impl<C> Handler<C> for fn(Request, &C, &mut Response) {
	fn handle_request(&self, request: Request, cache: &C, response: &mut Response) {
		(*self)(request, cache, response);
	}
}

impl<C: Cache, H: Handler<C>> Router<H> for H {
	fn find(&self, _method: &Method, _path: &str) -> Option<(&H, HashMap<String, String>)> {
		Some((self, HashMap::new()))
	}
}



///A trait for cache storage.
pub trait Cache {
	///Free all the unused cached resources.
	fn free_unused(&self);
}

impl Cache for () {
	fn free_unused(&self) {}
}



///A trait for request plugins.
///
///They are able to modify and react to a `Request` before it's sent to the handler.
#[experimental]
pub trait RequestPlugin {
	///Try to modify the `Request`.
	fn modify(&self, request: Request) -> RequestAction;
}

///A trait for response plugins.
///
///They are able to modify headers and data before it gets written in the response.
#[experimental]
pub trait ResponsePlugin {
	///Set or modify headers before they are sent to the client and maybe initiate the body.
	fn begin(&self, status: Status, headers: headers::response::HeaderCollection) ->
		(Status, headers::response::HeaderCollection, ResponseAction);

	///Handle content before writing it to the body.
	fn write<'a>(&'a self, content: Option<ResponseData<'a>>) -> ResponseAction;

	///End of body writing. Last chance to add content.
	fn end(&self) -> ResponseAction;
}



///Used to build and run a server.
///
///Each field has a corresponding modifier method and
///calls to these methods can be chained for quick setup.
///
///```no_run
///# use rustful::Server;
///# let router = ();
///Server::new().port(8080).handlers(router).run();
///```
pub struct Server<R, C> {
	///One or several response handlers.
	pub handlers: R,

	///The port where the server will listen for requests.
	pub port: Port,

	///The host address where the server will listen for requests.
	pub host: IpAddr,

	///The content of the server header.
	pub server: String,

	///The default media type.
	pub content_type: MediaType,

	///A structure where resources are cached.
	pub cache: C,

	///How often the cache should be cleaned. Measured in seconds.
	pub cache_clean_interval: Option<i64>,

	///The request plugin stack.
	#[experimental]
	pub request_plugins: Vec<Box<RequestPlugin + Send + Sync>>,

	///The response plugin stack.
	#[experimental]
	pub response_plugins: Vec<Box<ResponsePlugin + Send + Sync>>
}

impl Server<(), ()> {
	///Create a new empty server which will listen on host address `0.0.0.0` and port `80`.
	pub fn new() -> Server<(), ()> {
		Server {
			handlers: (),
			port: 80,
			host: Ipv4Addr(0, 0, 0, 0),
			server: "rustful".into_string(),
			content_type: MediaType {
				type_: String::from_str("text"),
				subtype: String::from_str("plain"),
				parameters: vec![(String::from_str("charset"), String::from_str("UTF-8"))]
			},
			cache: (),
			cache_clean_interval: None,
			request_plugins: Vec::new(),
			response_plugins: Vec::new(),
		}
	}
}

impl<R, C> Server<R, C> {
	///Set request handlers.
	pub fn handlers<NewRouter: Router<H>, H: Handler<C>>(self, handlers: NewRouter) -> Server<NewRouter, C> {
		Server {
			handlers: handlers,
			port: self.port,
			host: self.host,
			server: self.server,
			content_type: self.content_type,
			cache: self.cache,
			cache_clean_interval: self.cache_clean_interval,
			request_plugins: self.request_plugins,
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

	///Set resource cache.
	pub fn cache<NewCache>(self, cache: NewCache) -> Server<R, NewCache> {
		Server {
			handlers: self.handlers,
			port: self.port,
			host: self.host,
			server: self.server,
			content_type: self.content_type,
			cache: cache,
			cache_clean_interval: self.cache_clean_interval,
			request_plugins: self.request_plugins,
			response_plugins: self.response_plugins,
		}
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
	pub fn content_type(mut self, content_type: MediaType) -> Server<R, C> {
		self.content_type = content_type;
		self
	}

	///Add a request plugin to the plugin stack.
	#[experimental]
	pub fn with_request_plugin<P: RequestPlugin + Send + Sync>(mut self, plugin: P) ->  Server<R, C> {
		self.request_plugins.push(box plugin as Box<RequestPlugin + Send + Sync>);
		self
	}

	///Add a response plugin to the plugin stack.
	#[experimental]
	pub fn with_response_plugin<P: ResponsePlugin + Send + Sync>(mut self, plugin: P) ->  Server<R, C> {
		self.response_plugins.push(box plugin as Box<ResponsePlugin + Send + Sync>);
		self
	}
}

impl<R, H, C> Server<R, C>
	where
	R: Router<H> + Send + Sync,
	H: Handler<C> + Send + Sync,
	C: Cache + Send + Sync
{
	///Start the server and run forever.
	///This will only return if the initial connection fails.
	pub fn run(self) {
		let server = self.build();
		server.serve_forever();
	}

	///Build a runnable instance of the server.
	pub fn build(self) -> ServerInstance<R, C> {
		ServerInstance {
			handlers: Arc::new(self.handlers),
			port: self.port,
			host: self.host,
			server: self.server,
			content_type: self.content_type,
			cache: Arc::new(self.cache),
			cache_clean_interval: self.cache_clean_interval,
			last_cache_clean: Arc::new(RWLock::new(Timespec::new(0, 0))),
			request_plugins: Arc::new(self.request_plugins),
			response_plugins: Arc::new(self.response_plugins),
		}
	}
}

///A runnable instance of a server.
///
///It's not meant to be used directly,
///unless additional control is required.
///
///```no_run
///# use rustful::Server;
///# let router = ();
///let server_instance = Server::new().port(8080).handlers(router).build();
///```
pub struct ServerInstance<R, C> {
	handlers: Arc<R>,

	port: Port,
	host: IpAddr,

	server: String,
	content_type: MediaType,

	cache: Arc<C>,
	cache_clean_interval: Option<i64>,
	last_cache_clean: Arc<RWLock<Timespec>>,

	request_plugins: Arc<Vec<Box<RequestPlugin + Send + Sync>>>,
	response_plugins: Arc<Vec<Box<ResponsePlugin + Send + Sync>>>
}

impl<R, C> ServerInstance<R, C> {

	fn modify_request(&self, request: Request) -> RequestAction {
		let mut result = RequestAction::Continue(request);

		for plugin in self.request_plugins.iter() {
			result = match result {
				RequestAction::Continue(request) => plugin.modify(request),
				_ => return result
			};
		}

		result
	}

}

impl<R, H, C> http::server::Server for ServerInstance<R, C>
	where
	R: Router<H> + Send + Sync,
	H: Handler<C> + Send + Sync,
	C: Cache + Send + Sync,
{
	fn get_config(&self) -> Config {
		Config {
			bind_address: SocketAddr {
				ip: self.host,
				port: self.port
			}
		}
	}

	fn handle_request(&self, request: http::server::request::Request, writer: &mut ResponseWriter) {
		let http::server::request::Request {
			request_uri,
			method: request_method,
			headers: request_headers,
			body: request_body,
			..
		} = request;

		let mut response = Response::new(writer, self.response_plugins.deref());
		response.headers.date = Some(time::now_utc());
		response.headers.content_type = Some(self.content_type.clone());
		response.headers.server = Some(self.server.clone());

		let path_components = match request_uri {
			AbsoluteUri(url) => {
				Some((
					url.serialize_path().map(|p| p.into_bytes()).unwrap_or_else(|| vec!['/' as u8]),
					url.query_pairs().unwrap_or_else(|| Vec::new()).into_iter().collect(),
					url.fragment
				))
			},
			AbsolutePath(path) => {
				let (path, query, fragment) = parse_path(path);
				Some((path.into_bytes(), query, fragment))
			},
			_ => None //TODO: Handle *
		};

		match path_components {
			Some((path, query, fragment)) => {

				let request = Request {
					headers: request_headers,
					method: request_method,
					path: lossy_utf8_percent_decode(path.as_slice()),
					variables: HashMap::new(),
					query: query,
					fragment: fragment,
					body: request_body
				};

				match self.modify_request(request) {
					RequestAction::Continue(mut request) => {
						match self.handlers.find(&request.method, request.path.as_slice()) {
							Some((handler, variables)) => {
								request.variables = variables;
								handler.handle_request(request, & *self.cache, &mut response);
							},
							None => {
								response.headers.content_length = Some(0);
								response.status = NotFound;
							}
						}
					},
					RequestAction::Abort(status) => {
						response.headers.content_length = Some(0);
						response.status = status;
					}
				}
			},
			None => {
				response.headers.content_length = Some(0);
				response.status = BadRequest;
			}
		}

		//TODO: Maybe log errors here.
		match response.end() {
			_ => {}
		}

		self.cache_clean_interval.map(|t| {
			let clean_time = {
				let last_cache_clean = self.last_cache_clean.read();
				Timespec::new(last_cache_clean.sec + t, last_cache_clean.nsec)
			};

			if time::get_time() > clean_time {
				*self.last_cache_clean.write() = time::get_time();
				self.cache.free_unused();
			}
		});
	}
}

impl<R, C> Clone for ServerInstance<R, C>
	where
	R: Send + Sync,
	C: Send + Sync
{
	fn clone(&self) -> ServerInstance<R, C> {
		ServerInstance {
			handlers: self.handlers.clone(),
			port: self.port,
			host: self.host.clone(),
			server: self.server.clone(),
			content_type: self.content_type.clone(),
			cache: self.cache.clone(),
			cache_clean_interval: self.cache_clean_interval.clone(),
			last_cache_clean: self.last_cache_clean.clone(),
			request_plugins: self.request_plugins.clone(),
			response_plugins: self.response_plugins.clone()
		}
	}
}

fn parse_path(path: String) -> (String, HashMap<String, String>, Option<String>) {
	match path.as_slice().find('?') {
		Some(index) => {
			let (query, fragment) = parse_fragment(path.as_slice().slice(index+1, path.len()));
			(path.as_slice().slice(0, index).into_string(), utils::parse_parameters(query.as_bytes()), fragment.map(|f| f.into_string()))
		},
		None => {
			let (path, fragment) = parse_fragment(path.as_slice());
			(path.into_string(), HashMap::new(), fragment.map(|f| f.into_string()))
		}
	}
}

fn parse_fragment<'a>(path: &'a str) -> (&'a str, Option<&'a str>) {
	match path.find('#') {
		Some(index) => (path.slice(0, index), Some(path.slice(index+1, path.len()))),
		None => (path, None)
	}
}


///A container for all the request data, including get, set and path variables.
pub struct Request {
	///Headers from the HTTP request
	pub headers: headers::request::HeaderCollection,

	///The HTTP method
	pub method: Method,

	///The requested path
	pub path: String,

	///Route variables
	pub variables: HashMap<String, String>,

	///Query variables from the path
	pub query: HashMap<String, String>,

	///The fragment part of the URL (after #), if provided
	pub fragment: Option<String>,

	///The raw body part of the request
	pub body: Vec<u8>
}


///An interface for sending HTTP response data to the client.
pub struct Response<'a, 'b: 'a, 'c> {
	///The HTTP response headers. Date, content type (text/plain) and server is automatically set.
	pub headers: headers::response::HeaderCollection,

	///The HTTP response status. Ok (200) is default.
	pub status: Status,
	writer: &'a mut ResponseWriter<'b>,
	started_writing: bool,
	plugins: &'c Vec<Box<ResponsePlugin + Send + Sync>>
}

impl<'a, 'b, 'c> Response<'a, 'b, 'c> {
	fn new(writer: &'a mut ResponseWriter<'b>, plugins: &'c Vec<Box<ResponsePlugin + Send + Sync>>) -> Response<'a, 'b, 'c> {
		Response {
			headers: writer.headers.clone(), //Can't be borrowed, because writer must be borrowed
			status: status::Ok,
			writer: writer,
			started_writing: false,
			plugins: plugins
		}
	}

	///Writes a string or any other `BytesContainer` to the client.
	///The headers will be written the first time `send()` is called.
	pub fn send<'a, Content: IntoResponseData<'a>>(&mut self, content: Content) -> Result<(), ResponseError> {
		try!(self.begin());
		let mut plugin_result = ResponseAction::write(Some(content));

		for plugin in self.plugins.iter() {
			plugin_result = match plugin_result {
				Write(content) => plugin.write(content),
				_ => break
			}
		}

		let write_result = match plugin_result {
			Write(Some(ref s)) => Some(self.writer.write(s.as_bytes())),
			_ => None
		};

		match write_result {
			Some(Ok(_)) => Ok(()),
			Some(Err(e)) => Err(ResponseError::IoError(e)),
			None => match plugin_result {
				Error(e) => Err(ResponseError::PluginError(e)),
				Write(None) => Ok(()),
				_ => unreachable!()
			}
		}
	}

	///Start writing the response. Headers and status can not be changed after it has been called.
	///
	///This method will be called automatically by `send()` and `end()`, if it hasn't been called before.
	///It can only be called once.
	pub fn begin(&mut self) -> Result<(), ResponseError> {
		if !self.started_writing {
			self.started_writing = true;

			let mut write_queue = Vec::new();
			let mut header_result = (self.status.clone(), self.headers.clone(), Write(None));

			for plugin in self.plugins.iter() {
				header_result = match header_result {
					(_, _, DoNothing) => break,
					(_, _, Error(_)) => break,
					(status, headers, r) => {
						write_queue.push(r);

						match plugin.begin(status, headers) {
							(status, headers, Error(e)) => (status, headers, Error(e)),
							(status, headers, result) => {
								let mut error = None;
								
								write_queue = write_queue.into_iter().filter_map(|action| match action {
									Write(content) => Some(plugin.write(content)),
									DoNothing => None,
									Error(e) => {
										error = Some(e);
										None
									}
								}).collect();

								match error {
									Some(e) => (status, headers, Error(e)),
									None => (status, headers, result)
								}
							}
						}
					}
				}
			}

			match header_result {
				(_, _, Error(e)) => Err(ResponseError::PluginError(e)),
				(status, headers, last_result) => {
					write_queue.push(last_result);

					for action in write_queue.into_iter() {
						try!{
							match action {
								Write(Some(content)) => self.writer.write(content.as_bytes()),
								Error(e) => return Err(ResponseError::PluginError(e)),
								_ => Ok(())
							}
						}
					}

					self.writer.status = status;
					self.writer.headers = headers;
					Ok(())
				},
			}
		} else {
			Ok(())
		}
	}

	///Finish writing the response.
	pub fn end(&mut self) -> Result<(), ResponseError> {
		try!(self.begin());
		let mut write_queue: Vec<ResponseAction> = Vec::new();

		for plugin in self.plugins.iter() {
			let mut error = None;
			write_queue = write_queue.into_iter().filter_map(|action| match action {
				Write(content) => Some(plugin.write(content)),
				DoNothing => None,
				Error(e) => {
					error = Some(e);
					None
				}
			}).collect();

			match error {
				Some(e) => return Err(ResponseError::PluginError(e)),
				None => write_queue.push(plugin.end())
			}
		}

		for action in write_queue.into_iter() {
			try!{
				match action {
					Write(Some(content)) => self.writer.write(content.as_bytes()),
					Error(e) => return Err(ResponseError::PluginError(e)),
					_ => Ok(())
				}
			}
		}

		Ok(())
	}
}

impl<'a, 'b, 'c> Writer for Response<'a, 'b, 'c> {
	fn write(&mut self, content: &[u8]) -> IoResult<()> {
		match self.send(content) {
			Ok(()) => Ok(()),
			Err(ResponseError::IoError(e)) => Err(e),
			Err(ResponseError::PluginError(e)) => Err(std::io::IoError{
				kind: std::io::OtherIoError,
				desc: "response plugin error",
				detail: Some(e)
			})
		}
	}
}




#[test]
fn parse_path_parts() {
	let with = "this".into_string();
	let and = "that".into_string();
	let (path, query, fragment) = parse_path(String::from_str("/path/to/something?with=this&and=that#lol"));
	assert_eq!(path, String::from_str("/path/to/something"));
	assert_eq!(query.get(&"with".into_string()), Some(&with));
	assert_eq!(query.get(&"and".into_string()), Some(&and));
	assert_eq!(fragment, Some(String::from_str("lol")));
}

#[test]
fn parse_strange_path() {
	let with = "this".into_string();
	let and = "what?".into_string();
	let (path, query, fragment) = parse_path(String::from_str("/path/to/something?with=this&and=what?#"));
	assert_eq!(path, String::from_str("/path/to/something"));
	assert_eq!(query.get(&"with".into_string()), Some(&with));
	assert_eq!(query.get(&"and".into_string()), Some(&and));
	assert_eq!(fragment, Some(String::from_str("")));
}

#[test]
fn parse_missing_path_parts() {
	let with = "this".into_string();
	let and = "that".into_string();
	let (path, query, fragment) = parse_path(String::from_str("/path/to/something?with=this&and=that"));
	assert_eq!(path, String::from_str("/path/to/something"));
	assert_eq!(query.get(&"with".into_string()), Some(&with));
	assert_eq!(query.get(&"and".into_string()), Some(&and));
	assert_eq!(fragment, None);


	let (path, query, fragment) = parse_path(String::from_str("/path/to/something#lol"));
	assert_eq!(path, String::from_str("/path/to/something"));
	assert_eq!(query.len(), 0);
	assert_eq!(fragment, Some(String::from_str("lol")));


	let (path, query, fragment) = parse_path(String::from_str("?with=this&and=that#lol"));
	assert_eq!(path, String::from_str(""));
	assert_eq!(query.get(&"with".into_string()), Some(&with));
	assert_eq!(query.get(&"and".into_string()), Some(&and));
	assert_eq!(fragment, Some(String::from_str("lol")));
}