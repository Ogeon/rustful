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
Here is a simple example of how `my_project.rs` could look like:

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
fn handler(request: Request, response: &mut Response) {
	//Send something nice to the user
	try_send!(response, "Hello, user! It looks like this server works fine." while "sending hello");
}

fn main() {
	//Build and start the server. All code beyond this point is unreachable
	Server::new().port(8080).handlers(router!{"/" => Get: handler}).run();
}
```

##Contributing
Yes, please! This is currently a one man show, so any help is welcome. Especially ideas. The best way to
contribute, at the moment, is by giving me feedback or sending pull requests with fixes, corrections and useful/must-have
additions. Remember, when sending pull requests, that it is better to do a few things well, than many things
poorly.
