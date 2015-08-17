#[macro_use]
extern crate rustful;
use std::error::Error;
use rustful::{Server, Context, Response};

fn main() {
    println!("Visit http://localhost:8080 to try this example.");
    let server_result = Server {
        host: 8080.into(),
        ..Server::new(|_: Context, res: Response| res.send("Hello!"))
    }.run();

    match server_result {
        Ok(_server) => {},
        Err(e) => println!("could not start server: {}", e.description())
    }
}