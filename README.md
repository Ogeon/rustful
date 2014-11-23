rustful
=======

[![Build Status](https://travis-ci.org/Ogeon/rustful.png?branch=master)](https://travis-ci.org/Ogeon/rustful)

A RESTful web framework for Rust. The main purpose of rustful is to create a simple,
light and non-intrusive foundation for HTTP based applications. It is based on a stateless
structure, where response handlers are assigned to paths and HTTP methods, which naturally
allows it to run both as a single server or as multiple instances in a computer cluster.

Some of the features are:

* Generic response handlers. Just implement the Handler trait and you are done.
* Optional resource cache with lazy loading and simple cleaning of unused data.
* Some handy macros reduces risk for typos and makes life easier.
* Variables and recursive wildcards in routes.
* Minimal routing overhead.

Online documentation can be found [here](http://ogeon.github.io/rustful/doc/rustful/).

#Getting started

##Cargo.toml Entries

Add the following lines to your `Cargo.toml` file to get the main library:

```toml
[dependencies.rustful]
git = "https://github.com/Ogeon/rustful"
```

and the following lines to get all the helpful macros:

```toml
[dependencies.rustful_macros]
git = "https://github.com/Ogeon/rustful"
```

##Write your server
Here is a simple example of what a simple project could look like. Visit
`http://localhost:8080` or `http://localhost:8080/Olivia` (if your name is
Olivia) to try it.

```rust
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
```

##Contributing
Yes, please! This is currently a one man show, so any help is welcome. Just
fork it, implement your idea or fix the bug you have found and send a pull
request.
