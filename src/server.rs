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

use std::io::net::ip::{SocketAddr, Ipv4Addr, Port};
use std::hashmap::HashMap;

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
		AbsoluteUri(ref url) => Some(&url.path),
		AbsolutePath(ref path) => Some(path),
		_ => None
	};

	match path {
		Some(path) => {
			Some(Request {
				headers: request.headers.clone(),
				method: request.method.clone(),
				path: path.to_owned(),
				variables: ~HashMap::new()
			})
		}
		None => None
	}
}