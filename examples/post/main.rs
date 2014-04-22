extern crate rustful;
extern crate http;
use rustful::{Server, Router, Request, Response};
use rustful::response::MediaType;
use http::method::{Get, Post};

fn say_hello(request: &Request, response: &mut Response) {
	response.headers.content_type = Some(MediaType {
		type_: StrBuf::from_str("text"),
		subtype: StrBuf::from_str("html"),
		parameters: vec!((StrBuf::from_str("charset"), StrBuf::from_str("UTF-8")))
	});

	let content = match request.post.find(&~"name") {
		Some(name) => {
			format!("<p>Hello, {}!", name.to_str())
		},
		None => {
			~"<form method=\"post\"><div><label for=\"name\">Name: </label><input id=\"name\" type=\"text\" name=\"name\" /></div><div><input type=\"submit\" value=\"Say hello\" /></div></form>"
		}
	};

	match response.write(format!("<!DOCTYPE html><html charset=\"UTF-8\"><head><title>Hello</title></head><body>{}</body></html>", content).as_bytes()) {
		Err(e) => println!("error while writing hello: {}", e),
		_ => {}
	}
}

fn main() {
	let routes = [
		(Get, "/", say_hello),
		(Post, "/", say_hello)
	];

	let server = Server {
		handlers: ~Router::from_routes(routes),
		port: 8080
	};

	server.run();
}