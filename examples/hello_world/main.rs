#![feature(phase)]
#[phase(syntax)]
extern crate rustful;

extern crate rustful;
extern crate http;
use rustful::{Server, Router, Request, Response};

fn say_hello(request: &Request, response: &mut Response) {
	let person = match request.variables.find(&~"person") {
		Some(name) => name.to_str(),
		None => ~"stranger"
	};

	match response.write(format!("Hello, {}!", person).as_bytes()) {
		Err(e) => println!("error while writing hello: {}", e),
		_ => {}
	}
}

fn main() {
	let routes = routes!("/" => Get: say_hello, "/:person" => Get: say_hello);

	let server = Server {
		handlers: Router::from_routes(routes),
		port: 8080
	};

	server.run();
}