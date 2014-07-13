#![feature(phase)]
#[phase(plugin)]
extern crate rustful_macros;

extern crate rustful;
extern crate http;
use rustful::{Server, Router, Request, Response};
use http::method::Get;

fn say_hello(request: Request, response: &mut Response) {
	let person = match request.variables.find(&"person".into_string()) {
		Some(name) => name.as_slice(),
		None => "stranger"
	};

	try_send!(response, format!("Hello, {}!", person) while "showing hello");
}

fn main() {
	println!("Visit http://localhost:8080 or http://localhost:8080/Peter (if your name is Peter) to try this example.");

	/*let routes = routes!{"/" => Get: say_hello, "/:person" => Get: say_hello};

	let server = Server::new(8080, Router::from_routes(routes));*/

	//Temporary fix until functions can be cloned again
	let routes = router!{"/" => Get: say_hello, "/:person" => Get: say_hello};

	let server = Server::new(8080, routes);

	server.run();
}