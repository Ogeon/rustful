#![feature(phase)]
#[phase(plugin)]
extern crate rustful_macros;

extern crate rustful;
extern crate http;

use std::sync::RWLock;

use rustful::{Server, Router, Request, Response, RequestPlugin};
use rustful::{RequestResult, Continue};
use http::method::Get;

fn say_hello(request: Request, response: &mut Response) {
	let person = match request.variables.find(&"person".into_string()) {
		Some(name) => name.as_slice(),
		None => "stranger"
	};

	match response.send(format!("Hello, {}!", person)) {
		Err(e) => println!("error while writing hello: {}", e),
		_ => {}
	}
}

fn main() {
	println!("Visit http://localhost:8080 or http://localhost:8080/Peter (if your name is Peter) to try this example.");

	let routes = routes!{
		"print" => {
			Get: say_hello,
			":person" => Get: say_hello
		}
	};

	let mut server = Server::new(8080, Router::from_routes(routes));

	//Log path, change path, log again
	server.add_request_plugin(RequestLogger::new());
	server.add_request_plugin(PathPrefix::new("print"));
	server.add_request_plugin(RequestLogger::new());

	server.run();
}

struct RequestLogger {
	counter: RWLock<uint>
}

impl RequestLogger {
	pub fn new() -> RequestLogger {
		RequestLogger {
			counter: RWLock::new(0)
		}
	}
}

impl RequestPlugin for RequestLogger {
	///Count requests and log the path.
	fn modify(&self, request: Request) -> RequestResult {
		*self.counter.write() += 1;
		println!("Request #{} is to '{}'", *self.counter.read(), request.path);
		Continue(request)
	}
}


struct PathPrefix {
	prefix: &'static str
}

impl PathPrefix {
	pub fn new(prefix: &'static str) -> PathPrefix {
		PathPrefix {
			prefix: prefix
		}
	}
}

impl RequestPlugin for PathPrefix {
	///Append the prefix to the path
	fn modify(&self, request: Request) -> RequestResult {
		let mut request = request;
		request.path = format!("/{}{}", self.prefix.trim_chars('/'), request.path);
		Continue(request)
	}
}