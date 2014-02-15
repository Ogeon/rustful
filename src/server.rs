//! `Server` listens for HTTP requests.
//!
//!```rust
//!let server = Server {
//!	router: ~Router::from_vec(routes),
//!	port: 8080
//!};
//!
//!server.run();
//!```

use router::Router;
use http::server::{Request, ResponseWriter, Config};
use http::server::request::{AbsoluteUri, AbsolutePath, Star};
use http::headers::content_type::MediaType;
use std::io::net::ip::{SocketAddr, Ipv4Addr, Port};
use extra::time;

use HTTP = http::server::Server;

#[deriving(Clone)]
pub struct Server {
	///A routing tree with response handlers
	router: ~Router,

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

	fn handle_request(&self, request: &Request, writer: &mut ResponseWriter) {
		let response = match request.request_uri {
			Star => self.router.route(&"*"),
			AbsoluteUri(ref url) => self.router.route(url.path),
			AbsolutePath(ref path) => self.router.route(path.to_owned()),
			_ => None
		};

		let content = match response {
			Some(text) => text,
			None => ~"Error"
		}.as_bytes().to_owned();

		writer.headers.date = Some(time::now_utc());
		writer.headers.content_length = Some(content.len());
		writer.headers.content_type = Some(MediaType {
			type_: ~"text",
			subtype: ~"plain",
			parameters: ~[(~"charset", ~"UTF-8")]
		});
		writer.headers.server = Some(~"rustful");
		match writer.write(content) {
			Err(e) => println!("error when writing response: {}", e),
			_ => {}
		}
	}
}