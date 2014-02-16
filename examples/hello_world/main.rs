extern crate rustful;
use rustful::{Server, Router, Request};

fn say_hello(request: &Request) -> ~str {
	let person = match request.variables.find(&~"person") {
		Some(name) => name.to_str(),
		None => ~"stranger"
	};
	format!("Hello, {}!", person)
}

fn main() {
	let routes = ~[
		("/", say_hello),
		("/:person", say_hello)
	];

	let server = Server {
		router: ~Router::from_vec(routes),
		port: 8080
	};

	server.run();
}