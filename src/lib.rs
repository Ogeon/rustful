#![crate_id = "rustful#0.1-pre"]

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

use http::server::{ResponseWriter, Config, Server};
use http::server::request::{AbsoluteUri, AbsolutePath};
use http::method::{Post, Method};
use http::status;
use http::status::{NotFound, BadRequest, Status};
use http::headers::content_type::MediaType;
use http::headers;

use std::io::IoResult;
use std::io::net::ip::{SocketAddr, IpAddr, Ipv4Addr, Port};
use std::uint;
use std::io::BufReader;
use std::collections::hashmap::HashMap;

use sync::{Arc, RWLock};

use time::Timespec;


pub mod router;
pub mod cache;



///A trait for request handlers.
pub trait Handler<C> {
	fn handle_request(&self, request: &Request, cache: &C, response: &mut Response);
}

impl<C> Handler<C> for fn(&Request, &mut Response) {
	fn handle_request(&self, request: &Request, _cache: &C, response: &mut Response) {
		(*self)(request, response);
	}
}

impl<C> Handler<C> for fn(&Request, &C, &mut Response) {
	fn handle_request(&self, request: &Request, cache: &C, response: &mut Response) {
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



///Receives the HTTP requests and passes them on to handlers.
///
///```rust
///# use rustful::server::Server;
///# use rustful::router::Router;
///# let routes = [];
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
	last_cachel_clean: Arc<RWLock<Timespec>>
}

impl<H: Handler<()> + Send + Share> Server<H, ()> {
	///Create a new `Server` which will listen on the provided port and host address `0.0.0.0`.
	pub fn new(port: Port, handlers: Router<H>) -> Server<H, ()> {
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
			cache: Arc::new(()),
			cache_clean_interval: None,
			last_cachel_clean: Arc::new(RWLock::new(Timespec::new(0, 0)))
		}
	}
}

impl<H: Handler<C> + Send + Share, C: Cache + Send + Share> Server<H, C> {
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
			last_cachel_clean: Arc::new(RWLock::new(Timespec::new(0, 0)))
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

	///Change the server part of the user agent string.
	pub fn set_server_name(&mut self, name: String) {
		self.server = name;
	}

	///Change the default content type.
	pub fn set_content_type(&mut self, content_type: MediaType) {
		self.content_type = content_type;
	}
}

impl<H: Handler<C> + Send + Share, C: Cache + Send + Share> http::server::Server for Server<H, C> {
	fn get_config(&self) -> Config {
		Config {
			bind_address: SocketAddr {
				ip: self.host,
				port: self.port
			}
		}
	}

	fn handle_request(&self, request: &http::server::request::Request, writer: &mut ResponseWriter) {
		let mut response = Response::new(writer);
		response.headers.date = Some(time::now_utc());
		response.headers.content_type = Some(self.content_type.clone());
		response.headers.server = Some(self.server.clone());

		match get_path_components(request) {
			Some((path, query, fragment)) => {
				match self.handlers.find(request.method.clone(), path) {
					Some((handler, variables)) => {
						let post = if request.method == Post {
							parse_parameters(request.body.as_slice())
						} else {
							HashMap::new()
						};

						let request = Request {
							headers: *request.headers.clone(),
							method: request.method.clone(),
							path: path,
							variables: variables,
							post: post,
							query: query,
							fragment: fragment,
							body: request.body.as_slice()
						};

						handler.handle_request(&request, & *self.cache, &mut response);
					},
					None => {
						response.headers.content_length = Some(0);
						response.status = NotFound;
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
				let last_cachel_clean = self.last_cachel_clean.read();
				Timespec::new(last_cachel_clean.sec + t, last_cachel_clean.nsec)
			};

			if time::get_time() > clean_time {
				*self.last_cachel_clean.write() = time::get_time();
				self.cache.free_unused();
			}
		});
	}
}

impl<H: Send + Share, C: Send + Share> Clone for Server<H, C> {
	fn clone(&self) -> Server<H, C> {
		Server {
			handlers: self.handlers.clone(),
			port: self.port,
			host: self.host.clone(),
			server: self.server.clone(),
			content_type: self.content_type.clone(),
			cache: self.cache.clone(),
			cache_clean_interval: self.cache_clean_interval.clone(),
			last_cachel_clean: self.last_cachel_clean.clone()
		}
	}
}

fn get_path_components<'a>(request: &'a http::server::request::Request) -> Option<(&'a str, HashMap<String, String>, Option<&'a str>)> {
	match request.request_uri {
		AbsoluteUri(ref url) => {
			Some((
				url.path.as_slice(),
				url.query.iter().map(|&(ref k, ref v)| (k.clone(), v.clone()) ).collect(),
				url.fragment.as_ref().map(|v| v.as_slice())
			))
		},
		AbsolutePath(ref path) => Some(parse_path(path.as_slice())),
		_ => None
	}
}

fn parse_parameters(source: &str) -> HashMap<String, String> {
	let mut parameters = HashMap::new();
	for parameter in source.split('&') {
		let mut parts = parameter.split('=');
		parts.next().map(|name|
			parameters.insert(
				url_decode(name),
				parts.next().map(|v| url_decode(v)).unwrap_or_else(|| "".into_string())
			)
		);
	}

	parameters
}

fn parse_path<'a>(path: &'a str) -> (&'a str, HashMap<String, String>, Option<&'a str>) {
	match path.find('?') {
		Some(index) => {
			let (query, fragment) = parse_fragment(path.slice(index+1, path.len()));
			(path.slice(0, index), parse_parameters(query), fragment)
		},
		None => {
			let (path, fragment) = parse_fragment(path);
			(path, HashMap::new(), fragment)
		}
	}
}

fn parse_fragment<'a>(path: &'a str) -> (&'a str, Option<&'a str>) {
	match path.find('#') {
		Some(index) => (path.slice(0, index), Some(path.slice(index+1, path.len()))),
		None => (path, None)
	}
}

fn url_decode(string: &str) -> String {
	let mut rdr = BufReader::new(string.as_bytes());
	let mut out = Vec::new();

	loop {
		let mut buf = [0];
		let ch = match rdr.read(buf) {
			Err(..) => break,
			Ok(..) => buf[0] as char
		};
		match ch {
			'+' => out.push(' ' as u8),
			'%' => {
				let mut bytes = [0, 0];
				match rdr.read(bytes) {
					Ok(2) => {}
					_ => fail!()
				}
				let ch = uint::parse_bytes(bytes, 16u).unwrap() as u8;
				out.push(ch);
			}
			ch => out.push(ch as u8)
		}
	}

	String::from_utf8(out).unwrap_or_else(|_| string.into_string())
}


///A container for all the request data, including get, set and path variables.
pub struct Request<'a> {
	///Headers from the HTTP request
	pub headers: headers::request::HeaderCollection,

	///The HTTP method
	pub method: Method,

	///The requested path
	pub path: &'a str,

	///Route variables
	pub variables: HashMap<String, String>,

	///POST variables
	pub post: HashMap<String, String>,

	///Query variables from the path
	pub query: HashMap<String, String>,

	///The fragment part of the URL (after #), if provided
	pub fragment: Option<&'a str>,

	///The raw body part of the request
	pub body: &'a str
}


///An interface for sending HTTP response data to the client.
pub struct Response<'a, 'b> {
	///The HTTP response headers. Date, content type (text/plain) and server is automatically set.
	pub headers: headers::response::HeaderCollection,

	///The HTTP response status. Ok (200) is default.
	pub status: Status,
	writer: &'a mut ResponseWriter<'b>,
	started_writing: bool
}

impl<'a, 'b> Response<'a, 'b> {
	pub fn new<'a, 'b>(writer: &'a mut ResponseWriter<'b>) -> Response<'a, 'b> {
		Response {
			headers: *writer.headers.clone(), //Can't be borrowed, because writer must be borrowed
			status: status::Ok,
			writer: writer,
			started_writing: false
		}
	}

	///Start writing the response. Headers and status can not be changed after it has been called.
	///
	///This method will be called automatically by `write()` and `end()`, if it hasn't been called before.
	///It can only be called once.
	pub fn begin(&mut self) {
		if !self.started_writing {
			self.started_writing = true;
			//TODO: Intercept headers and status

			self.writer.status = self.status.clone();
			self.writer.headers = box self.headers.clone();

			//TODO: Begin content interception
		}
	}

	///Finish writing the response.
	pub fn end(&mut self) {
		self.begin();

		//TODO: End interception
	}
}

impl<'a, 'b> Writer for Response<'a, 'b> {
	///Writes content to the client. The headers will be written the first time it's called.
	fn write(&mut self, content: &[u8]) -> IoResult<()> {
		self.begin();

		//TODO: Intercept content

		self.writer.write(content)
	}
}



#[test]
fn parsing_parameters() {
	let parameters = parse_parameters("a=1&aa=2&ab=202");
	let a = "1".into_string();
	let aa = "2".into_string();
	let ab = "202".into_string();
	assert_eq!(parameters.find(&"a".into_string()), Some(&a));
	assert_eq!(parameters.find(&"aa".into_string()), Some(&aa));
	assert_eq!(parameters.find(&"ab".into_string()), Some(&ab));
}

#[test]
fn parsing_parameters_with_plus() {
	let parameters = parse_parameters("a=1&aa=2+%2B+extra+meat&ab=202+fifth+avenue");
	let a = "1".into_string();
	let aa = "2 + extra meat".into_string();
	let ab = "202 fifth avenue".into_string();
	assert_eq!(parameters.find(&"a".into_string()), Some(&a));
	assert_eq!(parameters.find(&"aa".into_string()), Some(&aa));
	assert_eq!(parameters.find(&"ab".into_string()), Some(&ab));
}

#[test]
fn parsing_strange_parameters() {
	let parameters = parse_parameters("a=1=2&=2&ab=");
	let a = "1".into_string();
	let aa = "2".into_string();
	let ab = "".into_string();
	assert_eq!(parameters.find(&"a".into_string()), Some(&a));
	assert_eq!(parameters.find(&"".into_string()), Some(&aa));
	assert_eq!(parameters.find(&"ab".into_string()), Some(&ab));
}

#[test]
fn parse_path_parts() {
	let with = "this".into_string();
	let and = "that".into_string();
	let (path, query, fragment) = parse_path("/path/to/something?with=this&and=that#lol");
	assert_eq!(path, "/path/to/something");
	assert_eq!(query.find(&"with".into_string()), Some(&with));
	assert_eq!(query.find(&"and".into_string()), Some(&and));
	assert_eq!(fragment, Some("lol"));
}

#[test]
fn parse_strange_path() {
	let with = "this".into_string();
	let and = "what?".into_string();
	let (path, query, fragment) = parse_path("/path/to/something?with=this&and=what?#");
	assert_eq!(path, "/path/to/something");
	assert_eq!(query.find(&"with".into_string()), Some(&with));
	assert_eq!(query.find(&"and".into_string()), Some(&and));
	assert_eq!(fragment, Some(""));
}

#[test]
fn parse_missing_path_parts() {
	let with = "this".into_string();
	let and = "that".into_string();
	let (path, query, fragment) = parse_path("/path/to/something?with=this&and=that");
	assert_eq!(path, "/path/to/something");
	assert_eq!(query.find(&"with".into_string()), Some(&with));
	assert_eq!(query.find(&"and".into_string()), Some(&and));
	assert_eq!(fragment, None);


	let (path, query, fragment) = parse_path("/path/to/something#lol");
	assert_eq!(path, "/path/to/something");
	assert_eq!(query.len(), 0);
	assert_eq!(fragment, Some("lol"));


	let (path, query, fragment) = parse_path("?with=this&and=that#lol");
	assert_eq!(path, "");
	assert_eq!(query.find(&"with".into_string()), Some(&with));
	assert_eq!(query.find(&"and".into_string()), Some(&and));
	assert_eq!(fragment, Some("lol"));
}