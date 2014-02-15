extern mod rustful;
use rustful::{Server, Router};
use std::hashmap::HashMap;

fn say_hello(variables: ~HashMap<~str, &str>) -> ~str {
	let person = match variables.find(&~"person") {
		Some(name) => name.to_str(),
		None => ~"stranger"
	};
	format!("Hello, {}!", person.to_str())
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