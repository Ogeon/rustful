#![feature(phase)]
#[phase(plugin)]
extern crate rustful_macros;

extern crate rustful;
extern crate http;

use std::sync::RWLock;

use rustful::{Server, Request, Response, RequestPlugin, ResponsePlugin};
use rustful::{RequestAction, Continue};
use rustful::{ResponseAction, ResponseData};
use http::method::Get;
use http::status::Status;
use http::headers::response::HeaderCollection;

fn say_hello(request: Request, _cache: &(), response: &mut Response) {
	let person = match request.variables.get(&"person".into_string()) {
		Some(name) => name.as_slice(),
		None => "stranger"
	};

	try_send!(response, format!("{{\"message\": \"Hello, {}!\"}}", person));
}

fn main() {
	println!("Visit http://localhost:8080 or http://localhost:8080/Peter (if your name is Peter) to try this example.");

	let router = router!{
		"print" => {
			Get: say_hello,
			":person" => Get: say_hello
		}
	};

	Server::new()
		   .handlers(router)
		   .port(8080)

			//Log path, change path, log again
		   .with_request_plugin(RequestLogger::new())
		   .with_request_plugin(PathPrefix::new("print"))
		   .with_request_plugin(RequestLogger::new())

		   .with_response_plugin(Jsonp::new("setMessage"))

		   .run();
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
	fn modify(&self, request: Request) -> RequestAction {
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
	fn modify(&self, request: Request) -> RequestAction {
		let mut request = request;
		request.path = format!("/{}{}", self.prefix.trim_chars('/'), request.path);
		Continue(request)
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
	fn begin(&self, status: Status, headers: HeaderCollection) -> (Status, HeaderCollection, ResponseAction) {
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