#![feature(phase)]
#[phase(plugin)]
extern crate rustful_macros;

extern crate rustful;

use std::sync::RWLock;
use std::borrow::ToOwned;

use rustful::{Server, TreeRouter, Request, Response, RequestPlugin, ResponsePlugin};
use rustful::RequestAction;
use rustful::RequestAction::Continue;
use rustful::{ResponseAction, ResponseData};
use rustful::Method::Get;
use rustful::StatusCode;
use rustful::header::Headers;

fn say_hello(request: Request, response: Response) {
	let person = match request.variables.get(&"person".to_owned()) {
		Some(name) => name.as_slice(),
		None => "stranger"
	};

	try_send!(response.into_writer(), format!("{{\"message\": \"Hello, {}!\"}}", person));
}

fn main() {
	println!("Visit http://localhost:8080 or http://localhost:8080/Peter (if your name is Peter) to try this example.");

	let mut router = TreeRouter::new();
	insert_routes!{
		&mut router: "print" => {
			Get: say_hello as fn(Request, Response),
			":person" => Get: say_hello as fn(Request, Response)
		}
	};

	let server_result = Server::new()
		   .handlers(router)
		   .port(8080)

			//Log path, change path, log again
		   .with_request_plugin(RequestLogger::new())
		   .with_request_plugin(PathPrefix::new("print"))
		   .with_request_plugin(RequestLogger::new())

		   .with_response_plugin(Jsonp::new("setMessage"))

		   .run();

	match server_result {
		Ok(_server) => {},
		Err(e) => println!("could not start server: {}", e)
	}
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
	fn modify(&self, request: &mut Request) -> RequestAction {
		*self.counter.write().unwrap() += 1;
		println!("Request #{} is to '{}'", *self.counter.read().unwrap(), request.path);
		Continue
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
	fn modify(&self, request: &mut Request) -> RequestAction {
		request.path = format!("/{}{}", self.prefix.trim_matches('/'), request.path);
		Continue
	}
}

struct Jsonp {
	function: &'static str
}

impl Jsonp {
	pub fn new(function: &'static str) -> Jsonp {
		Jsonp {
			function: function
		}
	}
}

impl ResponsePlugin for Jsonp {
	fn begin(&self, status: StatusCode, headers: Headers) -> (StatusCode, Headers, ResponseAction) {
		let action = ResponseAction::write(Some(format!("{}(", self.function)));
		(status, headers, action)
	}

	fn write<'a>(&'a self, bytes: Option<ResponseData<'a>>) -> ResponseAction {
		ResponseAction::write(bytes)
	}

	fn end(&self) -> ResponseAction {
		ResponseAction::write(Some(");"))
	}
}