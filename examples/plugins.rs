#![feature(plugin, core)]

#[plugin]
#[macro_use]
#[no_link]
extern crate rustful_macros;

extern crate rustful;

use std::sync::RwLock;
use std::borrow::ToOwned;
use std::error::Error;

use rustful::{Server, TreeRouter, Context, Response, Log};
use rustful::plugin::{ResponseAction, ContextPlugin, ResponsePlugin};
use rustful::plugin::ContextAction::{self, Continue};
use rustful::response::ResponseData;
use rustful::Method::Get;
use rustful::StatusCode;
use rustful::header::Headers;

fn say_hello(context: Context, response: Response) {
    let person = match context.variables.get(&"person".to_owned()) {
        Some(name) => &name[],
        None => "stranger"
    };

    if let Err(e) = response.into_writer().send(format!("{{\"message\": \"Hello, {}!\"}}", person))  {
        //There is not much we can do now
        context.log.note(&format!("could not send hello: {}", e.description()));
    }
}

fn main() {
    println!("Visit http://localhost:8080 or http://localhost:8080/Peter (if your name is Peter) to try this example.");

    let mut router = TreeRouter::new();
    insert_routes!{
        &mut router: "print" => {
            Get: say_hello,
            ":person" => Get: say_hello
        }
    };

    let server_result = Server::new()
           .handlers(router)
           .port(8080)

            //Log path, change path, log again
           .with_context_plugin(RequestLogger::new())
           .with_context_plugin(PathPrefix::new("print"))
           .with_context_plugin(RequestLogger::new())

           .with_response_plugin(Jsonp::new("setMessage"))

           .run();

    match server_result {
        Ok(_server) => {},
        Err(e) => println!("could not start server: {}", e.description())
    }
}

struct RequestLogger {
    counter: RwLock<u32>
}

impl RequestLogger {
    pub fn new() -> RequestLogger {
        RequestLogger {
            counter: RwLock::new(0)
        }
    }
}

impl ContextPlugin for RequestLogger {
    type Cache = ();

    ///Count requests and log the path.
    fn modify(&self, log: &Log, context: &mut Context) -> ContextAction {
        *self.counter.write().unwrap() += 1;
        if let Err(e) = log.note(&format!("Request #{} is to '{}'", *self.counter.read().unwrap(), context.path)) {
        	//This may not be the best way to handle a failed log,
        	//but it's fine in this example.
        	println!("could not log: {}", e);
        }
        Continue
    }
}


struct PathPrefix {
    prefix: &'static str
}

impl PathPrefix {
    pub fn new(prefix: &'static str) -> PathPrefix {
        PathPrefix {
            prefix: prefix
        }
    }
}

impl ContextPlugin for PathPrefix {
    type Cache = ();

    ///Append the prefix to the path
    fn modify(&self, _log: &Log, context: &mut Context) -> ContextAction {
        context.path = format!("/{}{}", self.prefix.trim_matches('/'), context.path);
        Continue
    }
}

struct Jsonp {
    function: &'static str
}

impl Jsonp {
    pub fn new(function: &'static str) -> Jsonp {
        Jsonp {
            function: function
        }
    }
}

impl ResponsePlugin for Jsonp {
    fn begin(&self, _log: &Log, status: StatusCode, headers: Headers) -> (StatusCode, Headers, ResponseAction) {
        let action = ResponseAction::write(Some(format!("{}(", self.function)));
        (status, headers, action)
    }

    fn write<'a>(&'a self, _log: &Log, bytes: Option<ResponseData<'a>>) -> ResponseAction {
        ResponseAction::write(bytes)
    }

    fn end(&self, _log: &Log) -> ResponseAction {
        ResponseAction::write(Some(");"))
    }
}