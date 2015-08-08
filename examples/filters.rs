#[macro_use]
extern crate rustful;

use std::sync::RwLock;
use std::error::Error;

use rustful::{Server, TreeRouter, Context, Response, Log, Handler};
use rustful::filter::{FilterContext, ResponseFilter, ResponseAction, ContextFilter, ContextAction};
use rustful::response::Data;
use rustful::StatusCode;
use rustful::header::Headers;
use rustful::context::Uri;

fn say_hello(mut context: Context, mut response: Response, format: &Format) {
    //Take the name of the JSONP function from the query variables
    let mut quote_msg = if let Some(jsonp_name) = context.query.remove("jsonp") {
        response.filter_storage_mut().insert(JsonpFn(jsonp_name));
        true
    } else {
        false
    };

    //Is the format supposed to be a JSON structure? Then set a variable name
    if let Format::Json = *format {
        response.filter_storage_mut().insert(JsonVar("message"));
        quote_msg = true;
    }

    let person = match context.variables.get("person") {
        Some(name) => &name[..],
        None => "stranger"
    };

    let message = if quote_msg {
        format!("\"Hello, {}!\"", person)
    } else {
        format!("Hello, {}!", person)
    };

    //Using `try_send` allows us to catch eventual errors from the filters.
    //This example should not produce any errors, so this is only for show.
    if let Err(e) = response.try_send(message) {
        context.log.note(&format!("could not send hello: {}", e.description()));
    }
}

enum Format {
    Json,
    Text
}

//Dodge an ICE, related to functions as handlers.
struct HandlerFn(fn(Context, Response, &Format), Format);

impl Handler for HandlerFn {
    fn handle_request(&self, context: Context, response: Response) {
        self.0(context, response, &self.1);
    }
}

fn main() {
    println!("Visit http://localhost:8080, http://localhost:8080/Peter or http://localhost:8080/json/Peter (if your name is Peter) to try this example.");
    println!("Append ?jsonp=someFunction to get a JSONP response.");

    let mut router = TreeRouter::new();
    insert_routes!{
        &mut router => {
            "print" => {
                Get: HandlerFn(say_hello, Format::Text),
                ":person" => Get: HandlerFn(say_hello, Format::Text),

                "json" => {
                    Get: HandlerFn(say_hello, Format::Json),
                    ":person" => Get: HandlerFn(say_hello, Format::Json)
                }
            }
        }
    };

    let server_result = Server {
        host: 8080.into(),
        handlers: router,

        //Log path, change path, log again
        context_filters: vec![
            Box::new(RequestLogger::new()),
            Box::new(PathPrefix::new("print")),
            Box::new(RequestLogger::new())
        ],

        response_filters: vec![Box::new(Jsonp), Box::new(Json)],

        ..Server::default()
    }.run();

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

impl ContextFilter for RequestLogger {
    ///Count requests and log the path.
    fn modify(&self, ctx: FilterContext, context: &mut Context) -> ContextAction {
        *self.counter.write().unwrap() += 1;
        ctx.log.note(&format!("Request #{} is to '{}'", *self.counter.read().unwrap(), context.uri));
        ContextAction::next()
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

impl ContextFilter for PathPrefix {
    ///Append the prefix to the path
    fn modify(&self, _ctx: FilterContext, context: &mut Context) -> ContextAction {
        let new_uri = context.uri.as_path().map(|path| {
            let mut new_path = vec!['/' as u8];
            //TODO: replace with push_all or whatever shows up
            new_path.extend(self.prefix.trim_matches('/').as_bytes().iter().cloned());
            new_path.extend(path.iter().cloned());
            Uri::Path(new_path)
        });
        if let Some(uri) = new_uri {
            context.uri = uri;
        }
        ContextAction::next()
    }
}

struct JsonVar(&'static str);

struct Json;

impl ResponseFilter for Json {
    fn begin(&self, ctx: FilterContext, status: StatusCode, _headers: &mut Headers) -> (StatusCode, ResponseAction) {
        //Check if a JSONP function is defined and write the beginning of the call
        let output = if let Some(&JsonVar(var)) = ctx.storage.get() {
            Some(format!("{{\"{}\": ", var))
        } else {
            None
        };

        (status, ResponseAction::next(output))
    }

    fn write<'a>(&'a self, _ctx: FilterContext, bytes: Option<Data<'a>>) -> ResponseAction {
        ResponseAction::next(bytes)
    }

    fn end(&self, ctx: FilterContext) -> ResponseAction {
        //Check if a JSONP function is defined and write the end of the call
        let output = ctx.storage.get::<JsonVar>().map(|_| "}");
        ResponseAction::next(output)
    }
}

struct JsonpFn(String);

struct Jsonp;

impl ResponseFilter for Jsonp {
    fn begin(&self, ctx: FilterContext, status: StatusCode, _headers: &mut Headers) -> (StatusCode, ResponseAction) {
        //Check if a JSONP function is defined and write the beginning of the call
        let output = if let Some(&JsonpFn(ref function)) = ctx.storage.get() {
            Some(format!("{}(", function))
        } else {
            None
        };

        (status, ResponseAction::next(output))
    }

    fn write<'a>(&'a self, _ctx: FilterContext, bytes: Option<Data<'a>>) -> ResponseAction {
        ResponseAction::next(bytes)
    }

    fn end(&self, ctx: FilterContext) -> ResponseAction {
        //Check if a JSONP function is defined and write the end of the call
        let output = ctx.storage.get::<JsonpFn>().map(|_| ");");
        ResponseAction::next(output)
    }
}
