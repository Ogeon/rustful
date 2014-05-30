//! `Server` listens for HTTP requests.
//!
//!```rust
//!# use rustful::server::Server;
//!# use rustful::router::Router;
//!# let routes = [];
//!let server = Server::new(8080, Router::from_routes(routes));
//!
//!server.run();
//!```

use router::Router;
use request::Request;
use response::Response;

use http;
use http::server::{ResponseWriter, Config, Server};
use http::server::request::{AbsoluteUri, AbsolutePath};
use http::method::Post;
use http::status::{NotFound, BadRequest};
use http::headers::content_type::MediaType;

use std::io::net::ip::{SocketAddr, IpAddr, Ipv4Addr, Port};
use std::uint;
use std::io::BufReader;

use sync::Arc;

use collections::hashmap::HashMap;

use time;


///A handler function for routing.
pub type HandlerFn = fn(&Request, &mut Response);

#[deriving(Clone)]
pub struct Server {
	///A routing tree with response handlers
	handlers: Arc<Router<HandlerFn>>,

	///The port where the server will listen for requests
	port: Port,

	///Host address
	host: IpAddr
}

impl Server {
	///Create a new `Server` which will listen on the provided port and host address `0.0.0.0`.
	pub fn new(port: Port, handlers: Router<HandlerFn>) -> Server {
		Server {
			handlers: Arc::new(handlers),
			port: port,
			host: Ipv4Addr(0, 0, 0, 0)
		}
	}

	///Start the server and run forever.
	///This will only return if the initial connection fails.
	pub fn run(self) {
		self.serve_forever();
	}

	///Change the host address.
	pub fn set_host(&mut self, host: IpAddr) {
		self.host = host;
	}
}

impl http::server::Server for Server {
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
		response.headers.content_type = Some(MediaType {
			type_: String::from_str("text"),
			subtype: String::from_str("plain"),
			parameters: vec![(String::from_str("charset"), String::from_str("UTF-8"))]
		});
		response.headers.server = Some(String::from_str("rustful"));

		match get_path_components(request) {
			Some((path, query, fragment)) => {
				match self.handlers.find(request.method.clone(), path) {
					Some((&handler, variables)) => {
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

						handler(&request, &mut response);
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