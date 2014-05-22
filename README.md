rustful
=======

[![Build Status](https://travis-ci.org/Ogeon/rustful.png?branch=master)](https://travis-ci.org/Ogeon/rustful)

A RESTful web framework for Rust. The main purpose of rustful is to create a simple,
light and non-intrusive foundation for HTTP based applications. It is based on a stateless
structure, where handler functions are assigned to paths and HTTP methods, which naturally
allows it to run both as a single server or as multiple instances in a computer cluster.

[Detailed documentation can be found here.](http://www.rust-ci.org/Ogeon/rustful/doc/rustful/)

#Getting started
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

This will put `libhttp*` and `librustful*` in the `my_project/lib/rustful/lib/` directory.
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
	match response.write("Hello, user! It looks like this server works fine.".as_bytes()) {
		Err(e) => println!("error while writing hello: {}", e),
		_ => {}
	}

}

fn main() {
	//Set the handler to respond to GET request for /
	let routes = [
		(Get, "/", handler)
	];

	let server = Server {
		//Build a router tree from the routes and give it to the server
		handlers: Router::from_routes(routes),

		//Set the listening port. Let's use 8080 this time
		port: 8080
	};

	//Start the server. All code beyond this point is unreachable
	server.run();
}
```

rustful comes with some handy macros to reduce some of the boilerplate code. The example above
may be rewritten using the `router!()` macro:

```rust
//Include rustful during both syntax and link phase to be able to use the macros
#![feature(phase)]
#[phase(syntax, link)] extern crate rustful;
extern crate http;
use rustful::{Server, Request, Response};
use http::method::Get;

///Our handler function
fn handler(request: &Request, response: &mut Response) {

	//Send something nice to the user
	match response.write("Hello, user! It looks like this server works fine.".as_bytes()) {
		Err(e) => println!("error while writing hello: {}", e),
		_ => {}
	}

}

fn main() {
	let server = Server {
		handlers: router!{"/" => Get: handler},

		//Set the listening port. Let's use 8080 this time
		port: 8080
	};

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
contribute, at the moment, is by giving me feedback or sending pull requests with fixes and useful/must-have
additions. Remember, when sending pull requests, that it is better to do a few things well, than many things
poorly.
