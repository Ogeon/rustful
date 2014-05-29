#![feature(phase)]
#[phase(syntax, link)] extern crate rustful;
extern crate http;
use rustful::{Server, Router, Request, Response};
use http::method::Get;

fn say_hello(request: &Request, response: &mut Response) {
	let person = match request.variables.find(&"person".into_string()) {
		Some(name) => name.as_slice(),
		None => "stranger"
	};

	match response.write(format!("Hello, {}!", person).as_bytes()) {
		Err(e) => println!("error while writing hello: {}", e),
		_ => {}
	}
}

fn main() {
	println!("Visit http://localhost:8080 or http://localhost:8080/Peter (if your name is Peter) to try this example.");

	let routes = routes!{"/" => Get: say_hello, "/:person" => Get: say_hello};

	let server = Server {
		handlers: Router::from_routes(routes),
		port: 8080
	};

	server.run();
}