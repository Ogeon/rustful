extern crate rustful;
extern crate http;
use rustful::{Server, Router, Request, Response};
use http::method::Get;

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
	let routes = ~[
		(Get, "/", say_hello),
		(Get, "/:person", say_hello)
	];

	let server = Server {
		router: ~Router::from_routes(routes),
		port: 8080
	};

	server.run();
}