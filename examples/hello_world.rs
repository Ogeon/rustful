//Include `rustful_macros` during the plugin phase
//to be able to use `router!` and `try_send!`.
#![feature(phase)]
#[phase(plugin)]
extern crate rustful_macros;

extern crate rustful;
extern crate http;
use rustful::{Server, Request, Response};
use http::method::Get;

fn say_hello(request: Request, response: &mut Response) {
    //Get the value of the path variable `:person`, from below.
    let person = match request.variables.get(&"person".into_string()) {
        Some(name) => name.as_slice(),
        None => "stranger"
    };

    //Use the value of the path variable to say hello.
    try_send!(response, format!("Hello, {}!", person) while "saying hello");
}

fn main() {
    println!("Visit http://localhost:8080 or http://localhost:8080/Olivia (if your name is Olivia) to try this example.");

    let router = router!{
        //Handle requests for root...
        "/" => Get: say_hello,

        //...and one level below.
        //`:person` is a path variable and it will be accessible in the handler.
        "/:person" => Get: say_hello
    };

    //Build and run the server. Anything below this point is unreachable.
    Server::new().port(8080).handlers(router).run();
}