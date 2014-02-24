rustful
=======

[![Build Status](https://travis-ci.org/Ogeon/rustful.png?branch=master)](https://travis-ci.org/Ogeon/rustful)

A RESTful web framework for Rust.

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
	let routes = ~[
		(Get, "/", handler)
	];

	let server = Server {
		//Build a router tree from the routes and give it to the server
		handlers: ~Router::from_routes(routes),

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