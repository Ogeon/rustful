//Include `rustful_macros` during the plugin phase
//to be able to use `router!` and `try_send!`.
#![feature(plugin)]

#[plugin]
#[macro_use]
#[no_link]
extern crate rustful_macros;

extern crate rustful;

use std::error::Error;

use rustful::{Server, Request, Response, TreeRouter};
use rustful::Method::Get;

fn say_hello(request: Request, _cache: &(), response: Response) {
    //Get the value of the path variable `:person`, from below.
    let person = match request.variables.get("person") {
        Some(name) => name.as_slice(),
        None => "stranger"
    };

    //Use the value of the path variable to say hello.
    try_send!(response.into_writer(), format!("Hello, {}!", person), "saying hello");
}

fn main() {
    println!("Visit http://localhost:8080 or http://localhost:8080/Olivia (if your name is Olivia) to try this example.");

    let router = insert_routes!{
        TreeRouter::new(): {
            //Handle requests for root...
            "/" => Get: say_hello,

            //...and one level below.
            //`:person` is a path variable and it will be accessible in the handler.
            "/:person" => Get: say_hello
        }
    };

    //Build and run the server.
    let server_result = Server::new().port(8080).handlers(router).run();

    match server_result {
        Ok(_server) => {},
        Err(e) => println!("could not start server: {}", e.description())
    }
}