#![crate_name = "rustful"]

#![comment = "RESTful web framework"]
#![license = "MIT"]
#![crate_type = "rlib"]

#![doc(html_root_url = "http://ogeon.github.io/rustful/doc/")]

#[cfg(test)]
extern crate test;

extern crate url;
extern crate time;
extern crate sync;
extern crate http;

pub use router::Router;

use http::server::Server as HttpServer;
use http::server::{ResponseWriter, Config};
use http::server::request::{AbsoluteUri, AbsolutePath};
use http::method::Method;
use http::status;
use http::status::{NotFound, BadRequest, Status};
use http::headers::content_type::MediaType;
use http::headers;

use std::io::IoResult;
use std::io::net::ip::{SocketAddr, IpAddr, Ipv4Addr, Port};
use std::collections::HashMap;
use std::path::BytesContainer;

use sync::{Arc, RWLock};

use time::Timespec;

use url::percent_encoding::lossy_utf8_percent_decode;

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
pub enum ResponseResult {
	///The response action was successful.
    Success,

    ///A response plugin failed.
    PluginError(String),

    ///There was an IO error.
    IoError(std::io::IoError)
}

///The result from a `ResponsePlugin`.
#[experimental]
pub enum ResponseAction<'a> {
	///Continue to the next plugin, but write nothing.
	WriteNothing,

	///Continue to the next plugin and write data in byte form.
	WriteBytes(Vec<u8>),

	///Continue to the next plugin and write data in byte form.
	WriteByteSlice(&'a [u8]),

	///Continue to the next plugin and write data in string form.
	WriteString(String),

	///Continue to the next plugin and write data in string form.
	WriteStringSlice(&'a str),

	///Do not continue to the next plugin.
	DoNothing,

	///Abort with an error.
	Error(String)
}

///A trait for request handlers.
pub trait Handler<C> {
	fn handle_request(&self, request: Request, cache: &C, response: &mut Response);
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
///They are able to modify headers and data before they are sent to the client.
#[experimental]
pub trait ResponsePlugin {
	///Set or modify headers before they are sent to the client and maybe initiate the body.
	fn begin(&self, status: Status, headers: headers::response::HeaderCollection) ->
		(Status, headers::response::HeaderCollection, ResponseAction);

	///Handle data from a byte vector.
	fn write_bytes(&self, content: Vec<u8>) -> ResponseAction;

	///End of body writing. Last chance to add content.
	fn end(&self) -> ResponseAction;


	///Handle data from a byte slice.
	fn write_byte_slice(&self, content: &[u8]) -> ResponseAction {
		self.write_bytes(content.to_vec())
	}

	///Handle data from a string.
	fn write_string(&self, content: String) -> ResponseAction {
		self.write_bytes(content.into_bytes())
	}

	///Handle data from a string slice.
	fn write_string_slice(&self, content: &str) -> ResponseAction {
		self.write_bytes(content.as_bytes().to_vec())
	}

	///What to do if `send` was called, but no data will be written.
	fn write_nothing(&self) -> ResponseAction {
		self.write_bytes(Vec::new())
	}
}



///Receives the HTTP requests and passes them on to handlers.
///
///```ignore
///# use rustful::Server;
///# use rustful::router::Router;
///# use http::method::Method;
///# let routes: &[(Method, &str, ()), ..0] = [].as_slice();
///let server = Server::new(8080, Router::from_routes(routes));
///
///server.run();
///```
pub struct Server<H, C> {
	///A router with response handlers
	handlers: Arc<Router<H>>,

	///The port where the server will listen for requests
	port: Port,

	///Host address
	host: IpAddr,

	server: String,
	content_type: MediaType,

	cache: Arc<C>,
	cache_clean_interval: Option<i64>,
	last_cache_clean: Arc<RWLock<Timespec>>,

	//An ASCII star will be rewarded to the one who sugests a better alternative to the RWLock.
	request_plugins: Arc<RWLock<Vec<Box<RequestPlugin + Send + Sync>>>>,
	response_plugins: Arc<RWLock<Vec<Box<ResponsePlugin + Send + Sync>>>>
}

impl<H: Handler<()> + Send + Sync> Server<H, ()> {
	///Create a new `Server` which will listen on the provided port and host address `0.0.0.0`.
	pub fn new(port: Port, handlers: Router<H>) -> Server<H, ()> {
		Server::with_cache(port, (), handlers)
	}
}

impl<H: Handler<C> + Send + Sync, C: Cache + Send + Sync> Server<H, C> {
	///Creates a new `Server` with a resource cache.
	///
	///The server will listen listen on the provided port and host address `0.0.0.0`.
	///Cache cleaning is disabled by default. 
	pub fn with_cache(port: Port, cache: C, handlers: Router<H>) -> Server<H, C> {
		Server {
			handlers: Arc::new(handlers),
			port: port,
			host: Ipv4Addr(0, 0, 0, 0),
			server: "rustful".into_string(),
			content_type: MediaType {
				type_: String::from_str("text"),
				subtype: String::from_str("plain"),
				parameters: vec![(String::from_str("charset"), String::from_str("UTF-8"))]
			},
			cache: Arc::new(cache),
			cache_clean_interval: None,
			last_cache_clean: Arc::new(RWLock::new(Timespec::new(0, 0))),
			request_plugins: Arc::new(RWLock::new(Vec::new())),
			response_plugins: Arc::new(RWLock::new(Vec::new())),
		}
	}

	///Start the server and run forever.
	///This will only return if the initial connection fails.
	pub fn run(self) {
		self.serve_forever();
	}
}

impl<H, C> Server<H, C> {
	///Change the host address.
	pub fn set_host(&mut self, host: IpAddr) {
		self.host = host;
	}

	///Set the minimal number of seconds between each cache clean.
	///
	///Passing `None` disables cache cleaning.
	pub fn set_clean_interval(&mut self, interval: Option<u32>) {
		self.cache_clean_interval = interval.map(|i| i as i64);
	}

	///Change the server response header.
	pub fn set_server_name(&mut self, name: String) {
		self.server = name;
	}

	///Change the default content type.
	pub fn set_content_type(&mut self, content_type: MediaType) {
		self.content_type = content_type;
	}

	///Add a request plugin to the plugin stack.
	#[experimental]
	pub fn add_request_plugin<P: RequestPlugin + Send + Sync>(&mut self, plugin: P) {
		self.request_plugins.write().push(box plugin as Box<RequestPlugin + Send + Sync>);
	}

	///Add a response plugin to the plugin stack.
	#[experimental]
	pub fn add_response_plugin<P: ResponsePlugin + Send + Sync>(&mut self, plugin: P) {
		self.response_plugins.write().push(box plugin as Box<ResponsePlugin + Send + Sync>);
	}

	fn modify_request(&self, request: Request) -> RequestAction {
		let mut result = Continue(request);

		for plugin in self.request_plugins.read().iter() {
			result = match result {
				Continue(request) => plugin.modify(request),
				_ => return result
			};
		}

		result
	}
}

impl<H: Handler<C> + Send + Sync, C: Cache + Send + Sync> http::server::Server for Server<H, C> {
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

		let plugins = self.response_plugins.read();
		let mut response = Response::new(writer, plugins.deref());
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
					Continue(mut request) => {
						match self.handlers.find(request.method.clone(), request.path.as_slice()) {
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
					Abort(status) => {
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

		response.end();

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

impl<H: Send + Sync, C: Send + Sync> Clone for Server<H, C> {
	fn clone(&self) -> Server<H, C> {
		Server {
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
	pub fn new(writer: &'a mut ResponseWriter<'b>, plugins: &'c Vec<Box<ResponsePlugin + Send + Sync>>) -> Response<'a, 'b, 'c> {
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
	pub fn send<S: BytesContainer>(&mut self, content: S) -> ResponseResult {
		match self.begin() {
			Success => {
				//TODO: Intercept content
				let mut plugin_result = match content.container_as_str() {
					Some(s) => WriteStringSlice(s),
					None => WriteByteSlice(content.container_as_bytes())
				};

				for plugin in self.plugins.iter() {
					plugin_result = match plugin_result {
						WriteBytes(b) => plugin.write_bytes(b),
						WriteByteSlice(b) => plugin.write_byte_slice(b),
						WriteString(s) => plugin.write_string(s),
						WriteStringSlice(s) => plugin.write_string_slice(s),
						WriteNothing => plugin.write_nothing(),
						_ => break
					}
				}

				let write_result = match plugin_result {
					WriteBytes(ref b) => Some(self.writer.write(b.as_slice())),
					WriteByteSlice(b) => Some(self.writer.write(b)),
					WriteString(ref s) => Some(self.writer.write(s.as_bytes())),
					WriteStringSlice(s) => Some(self.writer.write(s.as_bytes())),
					_ => None
				};

				match write_result {
					Some(r) => match r {
						Ok(_) => Success,
						Err(e) => IoError(e)
					},
					None => match plugin_result {
						Error(e) => PluginError(e),
						WriteNothing => Success,
						_ => unreachable!()
					}
				}
			}
			r => r
		}
	}

	///Start writing the response. Headers and status can not be changed after it has been called.
	///
	///This method will be called automatically by `send()` and `end()`, if it hasn't been called before.
	///It can only be called once.
	pub fn begin(&mut self) -> ResponseResult {
		if !self.started_writing {
			self.started_writing = true;

			let mut write_queue = Vec::new();
			let mut header_result = (self.status.clone(), self.headers.clone(), WriteNothing);

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
									WriteBytes(b) => Some(plugin.write_bytes(b)),
									WriteByteSlice(b) => Some(plugin.write_byte_slice(b)),
									WriteString(s) => Some(plugin.write_string(s)),
									WriteStringSlice(s) => Some(plugin.write_string_slice(s)),
									WriteNothing => Some(plugin.write_nothing()),
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
				(_, _, Error(e)) => PluginError(e),
				(status, headers, last_result) => {
					write_queue.push(last_result);

					for action in write_queue.into_iter() {
						let write_result = match action {
							WriteBytes(ref b) => self.writer.write(b.as_slice()),
							WriteByteSlice(b) => self.writer.write(b),
							WriteString(ref s) => self.writer.write(s.as_bytes()),
							WriteStringSlice(s) => self.writer.write(s.as_bytes()),
							Error(e) => return PluginError(e),
							_ => Ok(())
						};

						match write_result {
							Err(e) => return IoError(e),
							_ => {}
						}
					}

					self.writer.status = status;
					self.writer.headers = headers;
					Success
				},
			}
		} else {
			Success
		}
	}

	///Finish writing the response.
	pub fn end(&mut self) -> ResponseResult {
		match self.begin() {
			Success => {
				let mut write_queue: Vec<ResponseAction> = Vec::new();

				for plugin in self.plugins.iter() {
					let mut error = None;
					write_queue = write_queue.into_iter().filter_map(|action| match action {
						WriteBytes(b) => Some(plugin.write_bytes(b)),
						WriteByteSlice(b) => Some(plugin.write_byte_slice(b)),
						WriteString(s) => Some(plugin.write_string(s)),
						WriteStringSlice(s) => Some(plugin.write_string_slice(s)),
						WriteNothing => Some(plugin.write_nothing()),
						DoNothing => None,
						Error(e) => {
							error = Some(e);
							None
						}
					}).collect();

					match error {
						Some(e) => return PluginError(e),
						None => write_queue.push(plugin.end())
					}
				}

				for action in write_queue.into_iter() {
					let write_result = match action {
						WriteBytes(ref b) => self.writer.write(b.as_slice()),
						WriteByteSlice(b) => self.writer.write(b),
						WriteString(ref s) => self.writer.write(s.as_bytes()),
						WriteStringSlice(s) => self.writer.write(s.as_bytes()),
						Error(e) => return PluginError(e),
						_ => Ok(())
					};

					match write_result {
						Err(e) => return IoError(e),
						_ => {}
					}
				}

				Success
			},
			r => r
		}
	}
}

impl<'a, 'b, 'c> Writer for Response<'a, 'b, 'c> {
	fn write(&mut self, content: &[u8]) -> IoResult<()> {
		match self.send(content) {
			Success => Ok(()),
			IoError(e) => Err(e),
			PluginError(e) => Err(std::io::IoError{
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