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
The following example server is very trivial and more advanced servers can be
found in the `examples/` directory.

Let's assume your file structure looks like this:

```
my_project/
    lib/
    src/
        my_project.rs
```

##Building rustful
First you need to get the latest rustful from GitHub:
```shell
cd lib
git clone --recursive https://github.com/Ogeon/rustful.git
```

Your file structure will now look like this:

```
my_project/
    lib/
        rustful/
    src/
        my_project.rs
```

Now, go into `rustful/` and build the library

```shell
cd rustful
make deps
make
```

This will put `libhttp*`, `librustful*` and `librustful_macros*` in the `my_project/lib/rustful/lib/` directory.
The `rustful` and `rustful_macros` crates can be built separately by running `make rustful` and `make macros`.
You can also run `make docs` to generate documentation and `make examples` to build the examples.

See the rust-http README for information about SSL support.

##Write your server
Here is a simple example of how `my_project.rs` could look like:
```rust
extern crate rustful;
extern crate http;
use rustful::{Server, Router, Request, Response};
use http::method::Get;

///Our handler function
fn handler(request: &Request, response: &mut Response) {

	//Send something nice to the user
	match response.send("Hello, user! It looks like this server works fine.") {
		Err(e) => println!("error while writing hello: {}", e),
		_ => {}
	}

}

fn main() {
	//Set the handler to respond to GET request for /
	let routes = [
		(Get, "/", handler)
	];

	let server = Server::new(
		//Set the listening port. Let's use 8080 this time
		8080,

		//Build a router tree from the routes and give it to the server
		Router::from_routes(routes)
	);

	//Start the server. All code beyond this point is unreachable
	server.run();
}
```

rustful comes with some handy macros to reduce some of the boilerplate code. The example above
may be rewritten using the `router!()` macro:

```rust
//Include rustful_macros during syntax phase to be able to use the macros
#![feature(phase)]
#[phase(plugin)]
extern crate rustful_macros;

extern crate rustful;
extern crate http;
use rustful::{Server, Request, Response};
use http::method::Get;

///Our handler function
fn handler(request: &Request, response: &mut Response) {

	//Send something nice to the user
	match response.send("Hello, user! It looks like this server works fine.") {
		Err(e) => println!("error while writing hello: {}", e),
		_ => {}
	}

}

fn main() {
	let server = Server::new(8080, router!{"/" => Get: handler});

	//Start the server. All code beyond this point is unreachable
	server.run();
}
```

##Compile and run your server
Just run these commands from the project root (`my_project/`):
```shell
rustc -L lib/rustful/lib/ -o my_server src/my_server.rs
./my_server
```

##Contributing
Yes, please! This is currently a one man show, so any help is welcome. Especially ideas. The best way to
contribute, at the moment, is by giving me feedback or sending pull requests with fixes, corrections and useful/must-have
additions. Remember, when sending pull requests, that it is better to do a few things well, than many things
poorly.
