#![feature(phase)]
#[phase(syntax)]
extern crate rustful;

extern crate rustful;
extern crate http;
use rustful::{Server, Request, Response};
use http::method::{Get, Post};

fn say_hello(request: &Request, response: &mut Response) {
	response.headers.content_type = content_type!("text", "html", "charset": "UTF-8");

	let content = match request.post.find(&"name".into_strbuf()) {
		Some(name) => {
			format!("<p>Hello, {}!</p>", name.to_str())
		},
		None => {
			"<form method=\"post\">\
				<div>\
					<label for=\"name\">Name: </label><input id=\"name\" type=\"text\" name=\"name\" />\
				</div>\
				<div>\
					<input type=\"submit\" value=\"Say hello\" />\
				</div>\
			</form>".to_owned()
		}
	};

	match response.write(format!("<!DOCTYPE html><html charset=\"UTF-8\"><head><title>Hello</title></head><body>{}</body></html>", content).as_bytes()) {
		Err(e) => println!("error while writing hello: {}", e),
		_ => {}
	}
}

fn main() {
	println!("Visit http://localhost:8080 to try this example.");

	let server = Server {
		handlers: router!{"/" => Get | Post: say_hello},
		port: 8080
	};

	server.run();
}