#[macro_use]
extern crate log;
extern crate env_logger;

extern crate rustful;
use std::error::Error;
use rustful::{Server, Context};

fn main() {
    env_logger::init().unwrap();

    println!("Visit http://localhost:8080 to try this example.");
    let server_result = Server {
        host: 8080.into(),
        ..Server::new(|_: &mut Context| "Hello!")
    }.run();

    match server_result {
        Ok(_server) => {},
        Err(e) => error!("could not start server: {}", e.description())
    }
}
