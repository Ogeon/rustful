//Include `rustful_macros` during the plugin phase
//to be able to use `router!`.
#![feature(plugin)]
#![plugin(rustful_macros)]

extern crate rustful;

use std::error::Error;

use rustful::{Server, Context, Response, TreeRouter};
use rustful::Method::Get;

fn say_hello(context: Context, response: Response) {
    //Get the value of the path variable `:person`, from below.
    let person = match context.variables.get("person") {
        Some(name) => &name[],
        None => "stranger"
    };

    //Use the value of the path variable to say hello.
    if let Err(e) = response.into_writer().send(format!("Hello, {}!", person))  {
        //There is not much we can do now
        context.log.note(&format!("could not send hello: {}", e.description()));
    }
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