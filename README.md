Rustful
=======

[![Join the chat at https://gitter.im/Ogeon/rustful](https://badges.gitter.im/Join%20Chat.svg)](https://gitter.im/Ogeon/rustful?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge&utm_content=badge)

[![Build Status](https://travis-ci.org/Ogeon/rustful.png?branch=master)](https://travis-ci.org/Ogeon/rustful)
[![Windows Build status](https://ci.appveyor.com/api/projects/status/6a95paoex0eptbgn/branch/master?svg=true)](https://ci.appveyor.com/project/Ogeon/rustful/branch/master)

A light HTTP framework for Rust, with REST-like features. The main purpose
of Rustful is to create a simple, modular and non-intrusive foundation for
HTTP applications. It has a mainly stateless structure, which naturally allows
it to run both as one single server and as multiple instances in a cluster.

Some of the features are:

* Generic response handlers. Just use a function or implement the Handler trait.
* Some handy macros reduces the risk for typos and makes life easier.
* Variables in routes, that can capture parts of the requested path.
* Pluggable request and response filtering.

[Online documentation](http://ogeon.github.io/docs/rustful/master/rustful/index.html).

#Getting Started

##Cargo.toml Entries

Add the following lines to your `Cargo.toml` file:

```toml
[dependencies]
rustful = "0.6"
```

###Cargo Features

Some parts of Rustful can be toggled using Cargo features:

 * `rustc_json_body` - Parse the request body as JSON. Enabled by default.
 * `ssl` - Enable SSL, and thereby HTTPS. Enabled by default.
 * `multipart` - Enable parsing of `multipart/form-data` requests. Enabled by default.

###Using SSL
Note that the `ssl` feature requires OpenSSL to be installed in one way or
another. See https://github.com/sfackler/rust-openssl#building for more
instructions.

##Write Your Server
Here is a simple example of what a simple project could look like. Visit
`http://localhost:8080` or `http://localhost:8080/Olivia` (if your name is
Olivia) to try it.

```rust
//Include macros to be able to use `insert_routes!`.
#[macro_use]
extern crate rustful;

#[macro_use]
extern crate log;
extern crate env_logger;

use std::error::Error;

use rustful::{Server, Context, Response, TreeRouter};

fn say_hello(context: Context, response: Response) {
    //Get the value of the path variable `:person`, from below.
    let person = match context.variables.get("person") {
        Some(name) => name,
        None => "stranger".into()
    };

    //Use the name from the path variable to say hello.
    response.send(format!("Hello, {}!", person));
}

fn main() {
    env_logger::init().unwrap();

    //Build and run the server.
    let server_result = Server {
        //Turn a port number into an IPV4 host address (0.0.0.0:8080 in this case).
        host: 8080.into(),

        //Create a TreeRouter and fill it with handlers.
        handlers: insert_routes!{
            TreeRouter::new() => {
                //Handle requests for root...
                Get: say_hello,

                //...and one level below.
                //`:person` is a path variable and it will be accessible in the handler.
                ":person" => Get: say_hello
            }
        },

        //Use default values for everything else.
        ..Server::default()
    }.run();

    match server_result {
        Ok(_server) => {},
        Err(e) => error!("could not start server: {}", e.description())
    }
}
```

##Contributing

Contributions are always welcome, even if it's a small typo fix (or maybe I
should say "especially typo fixes"). You can fork the project and open a pull
request with your changes, or create an issue if you just want to report or
request something. Are you not sure about how to implement your change? Is it
still a work in progress? Don't worry. You can still open a pull request where
we can discuss it and do it step by step.

New features are as welcome as fixes, so pull requests and proposals with
enhancements are very much appreciated, but please explain your feature and
give a good motivation to why it should be included. It makes things much
easier, both for reviewing the feature and for those who are not as familiar
with how things work. You can always open an issue where we can discuss the
feature and see if it should be included. Asking is better than assuming!

###Testing

Rustful is tested on Linux, using Travis, and on Windows, using AppVeyor and a
pull request will not be approved unless it passes these tests. It is
therefore a good idea to run tests locally, before pushing your changes, so here
is a small list of useful commands:

 * `cargo test` - Basic unit, documentation and compile tests.
 * `cargo build --no-default-features` - Check if the most minimal version of Rustful builds.
 * `cargo build --no-default-features --features "feature1 feature2"` - Check if Rustful with only `feature1` and `feature2` enabled builds.
 * `cargo run --example example_name` - check if the example `example_name` behaves as expected (see the `example` directory).

Travis and AppVeyor will run the tests with the `strict` feature enabled. This
turns warnings and missing documentation into compile errors, which may be
harsh, but it's for the sake of the user. Everything should have a description
and it's not nice to see warnings from your dependencies when you are
compiling your project, right? It's therefore recommend that you run your own
tests with the `strict` feature enabled before pushing, just to see if you
missed something.

###Automatic Feature Testing

User facing Cargo features are automatically gathered from `Cargo.toml` and
tested one at the time, using `scripts/test_features.sh`. The lack of public
and private features forces us to use a special annotation to differ between
internal and user facing feature. Here is an simple example snippet of how the
`Cargo.toml` is expected to look:

```toml
#...

[features]
default = ["feature_a", "feature_b"]
feature_a = ["feature_c"]
feature_b = []

#internal
feature_c = []

[dependencies.optional_lib]
#feature
optional=true

#...
```

Features that are supposed to be available to the user has to be declared
before the `#internal` comment. This will tell the test script that these are
supposed to be tested.

Dependency libraries can also be features, so we have to annotate these as
well. Each dependency that is supposed to work as a user facing feature will
need a `#feature` comment somewhere within its declaration. This will only
work with features that are declared using the above form, and not the
`feature_lib = { ... }` form.
