//! `Server` listens for HTTP requests.
//!
//!```rust
//!# use rustful::server::Server;
//!# use rustful::router::Router;
//!# let routes = [];
//!let server = Server {
//!	router: ~Router::from_routes(routes),
//!	port: 8080
//!};
//!
//!server.run();
//!```

use router::Router;
use request::Request;
use response::Response;
use response::status::NotFound;

use http;
use http::server::{ResponseWriter, Config};
use http::server::request::{AbsoluteUri, AbsolutePath};
use http::headers::content_type::MediaType;
use http::method::Post;

use std::io::net::ip::{SocketAddr, Ipv4Addr, Port};
use std::hashmap::HashMap;
use std::str::from_utf8;
use std::uint;
use std::io::BufReader;

use extra::time;

use HTTP = http::server::Server;


///A handler function for routing.
pub type HandlerFn = fn(&Request, &mut Response);

#[deriving(Clone)]
pub struct Server {
	///A routing tree with response handlers
	router: ~Router<HandlerFn>,

	///The port where the server will listen for requests
	port: Port
}

impl Server {
	///Start the server and run forever.
	///This will only return if the initial connection fails.
	pub fn run(&self) {
		self.clone().serve_forever();
	}
}

impl HTTP for Server {
	fn get_config(&self) -> Config {
		Config {
			bind_address: SocketAddr {
				ip: Ipv4Addr(0, 0, 0, 0),
				port: self.port
			}
		}
	}

	fn handle_request(&self, request: &http::server::request::Request, writer: &mut ResponseWriter) {
		let mut response = Response::new(writer);
		response.headers.date = Some(time::now_utc());
		response.headers.content_type = Some(MediaType {
			type_: ~"text",
			subtype: ~"plain",
			parameters: ~[(~"charset", ~"UTF-8")]
		});
		response.headers.server = Some(~"rustful");

		let found = match build_request(request) {
			Some(mut request) => match self.router.find(request.method.clone(), request.path) {
				Some((&handler, variables)) => {
					request.variables = variables;
					handler(&request, response);
					true
				},
				None => false
			},
			None => false
		};
		
		if !found {
			let content = bytes!("File not found");
			response.headers.content_length = Some(content.len());
			response.status = NotFound;
			match response.write(content.to_owned()) {
				Err(e) => println!("error while writing 404: {}", e),
				_ => {}
			}
		}
	}
}

fn build_request(request: &http::server::request::Request) -> Option<Request> {
	let path = match request.request_uri {
		AbsoluteUri(ref url) => {
			let query = ~url.query.iter().map(|&(ref a, ref b)| {
				(a.to_str(), b.to_str())
			}).collect();
			Some((url.path.to_str(), query, url.fragment.to_str()))
		},
		AbsolutePath(ref path) => Some(parse_path(path.to_str())),
		_ => None
	};

	match path {
		Some((path, query, fragment)) => {
			let post = if request.method == Post {
				parse_parameters(request.body)
			} else {
				~HashMap::new()
			};

			Some(Request {
				headers: request.headers.clone(),
				method: request.method.clone(),
				path: path.to_owned(),
				variables: ~HashMap::new(),
				post: post,
				query: query,
				fragment: fragment,
				body: request.body.to_str()
			})
		}
		None => None
	}
}

fn parse_parameters(source: &str) -> ~HashMap<~str, ~str> {
	let mut parameters = ~HashMap::new();
	for parameter in source.split('&') {
		let parts = parameter.split('=').collect::<~[&str]>();
		match parts {
			[name, value] => {
				parameters.insert(url_decode(name), url_decode(value));
			},
			[name] => {
				parameters.insert(url_decode(name), ~"");
			},
			[name, value, ..] => {
				parameters.insert(url_decode(name), url_decode(value));
			},
			[] => {}
		}
	}

	parameters
}

fn parse_path(path: &str) -> (~str, ~HashMap<~str, ~str>, ~str) {
	match path.find('?') {
		Some(index) => {
			let (query, fragment) = parse_fragment(path.slice(index+1, path.len()));
			(path.slice(0, index).to_str(), parse_parameters(query), fragment)
		},
		None => {
			let (path, fragment) = parse_fragment(path);
			(path, ~HashMap::new(), fragment)
		}
	}
}

fn parse_fragment(path: &str) -> (~str, ~str) {
	match path.find('#') {
		Some(index) => (path.slice(0, index).to_str(), path.slice(index+1, path.len()).to_str()),
		None => (path.to_str(), ~"")
	}
}

fn url_decode(string: &str) -> ~str {
	let mut rdr = BufReader::new(string.as_bytes());
	let mut out = ~[];

	loop {
		let mut buf = [0];
		let ch = match rdr.read(buf) {
			Err(..) => break,
			Ok(..) => buf[0] as char
		};
		match ch {
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

	match from_utf8(out) {
		Some(result) => result.to_str(),
		None => string.to_str()
	}
}

#[test]
fn parsing_parameters() {
	let parameters = parse_parameters("a=1&aa=2&ab=202");
	let a = ~"1";
	let aa = ~"2";
	let ab = ~"202";
	assert_eq!(parameters.find(&~"a"), Some(&a));
	assert_eq!(parameters.find(&~"aa"), Some(&aa));
	assert_eq!(parameters.find(&~"ab"), Some(&ab));
}

#[test]
fn parsing_strange_parameters() {
	let parameters = parse_parameters("a=1=2&=2&ab=");
	let a = ~"1";
	let aa = ~"2";
	let ab = ~"";
	assert_eq!(parameters.find(&~"a"), Some(&a));
	assert_eq!(parameters.find(&~""), Some(&aa));
	assert_eq!(parameters.find(&~"ab"), Some(&ab));
}

#[test]
fn parse_path_parts() {
	let (path, query, fragment) = parse_path("/path/to/something?whith=this&and=that#lol");
	assert_eq!(path, ~"/path/to/something");
	assert_eq!(query, ~"whith=this&and=that");
	assert_eq!(fragment, ~"lol");
}

#[test]
fn parse_strange_path() {
	let (path, query, fragment) = parse_path("/path/to/something?whith=this&and=what?#");
	assert_eq!(path, ~"/path/to/something");
	assert_eq!(query, ~"whith=this&and=what?");
	assert_eq!(fragment, ~"");
}

#[test]
fn parse_missing_path_parts() {
	let (path, query, fragment) = parse_path("/path/to/something?whith=this&and=that");
	assert_eq!(path, ~"/path/to/something");
	assert_eq!(query, ~"whith=this&and=that");
	assert_eq!(fragment, ~"");


	let (path, query, fragment) = parse_path("/path/to/something#lol");
	assert_eq!(path, ~"/path/to/something");
	assert_eq!(query, ~"");
	assert_eq!(fragment, ~"lol");


	let (path, query, fragment) = parse_path("?whith=this&and=that#lol");
	assert_eq!(path, ~"");
	assert_eq!(query, ~"whith=this&and=that");
	assert_eq!(fragment, ~"lol");
}