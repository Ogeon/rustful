#[macro_use]
extern crate rustful;

use std::sync::RwLock;
use std::error::Error;
use std::borrow::Cow;

#[macro_use]
extern crate log;
extern crate env_logger;

use rustful::{Server, DefaultRouter, Context, Response, ResponseParams};
use rustful::filter::{FilterContext, ContextFilter, ContextAction};
use rustful::header::ContentType;
use rustful::context::{UriPath, MaybeUtf8Owned};
use rustful::handler::Sequence;

fn say_hello(context: &mut Context, format: Format) -> (Format, ResponseParams<String>) {
    let person = match context.variables.get("person") {
        Some(name) => name,
        None => "stranger".into()
    };

    (format, ResponseParams::new(format!("Hello, {}!", person)))
}

fn main() {
    env_logger::init().unwrap();

    println!("Visit http://localhost:8080, http://localhost:8080/Peter or http://localhost:8080/json/Peter (if your name is Peter) to try this example.");
    println!("Append ?jsonp=someFunction to get a JSONP response.");
    println!("Run this example with the environment variable 'RUST_LOG' set to 'debug' to see the debug prints.");

    let mut router = DefaultRouter::<Sequence<_>>::new();
    router.build().path("print").many(|mut node| {
        node.then().on_get(make_sequence(say_hello, Format::Text));
        node.path(":person").then().on_get(make_sequence(say_hello, Format::Text));

        node.path("json").many(|mut node| {
            node.then().on_get(make_sequence(say_hello, Format::Json));
            node.path(":person").then().on_get(make_sequence(say_hello, Format::Json));
        });
    });

    let server_result = Server {
        host: 8080.into(),
        handlers: router,

        //Log path, change path, log again
        context_filters: vec![
            Box::new(RequestLogger::new()),
            Box::new(PathPrefix::new("print")),
            Box::new(RequestLogger::new())
        ],

        content_type: content_type!(Text / Plain; Charset = Utf8),

        ..Server::default()
    }.run();

    match server_result {
        Ok(_server) => {},
        Err(e) => error!("could not start server: {}", e.description())
    }
}

#[derive(Copy, Clone, PartialEq)]
enum Format {
    Json,
    Text
}

///Append content filters to a handler function.
fn make_sequence(handler: fn(&mut Context, Format) -> (Format, ResponseParams<String>), format: Format) -> Sequence<ResponseParams<String>> {
    Sequence::build(move |context: &mut Context, _: &Response| handler(context, format))
        .then(json_format)
        .then(jsonp_format)
        .done()
}

///Reformat the output as a JSON object if `format == Format::Json`.
fn json_format(_: &mut Context, _: &Response, (format, mut response): (Format, ResponseParams<String>)) -> (Format, ResponseParams<String>) {
    if format == Format::Json {
        //Wrap the content in a JSON object
        let output = format!("{{\"message\": \"{}\"}}", response.inner());
        *response.inner_mut() = output;
        response.require_header(ContentType(content_type!(Application / Json; Charset = Utf8)));
    };

    (format, response)
}

///Reformat the output as a JavaScript function call (JSONP) if a
///`?jsonp=<function>` query parameter has been added to the URL.
fn jsonp_format(context: &mut Context, _: &Response, (format, mut response): (Format, ResponseParams<String>)) -> ResponseParams<String> {
    //Take the name of the JSONP function from the query variables
    if let Some(function) = context.query.get("jsonp") {
        let output = {
            //Add quotes, even if not under /json
            let content = if format == Format::Text {
                Cow::from(format!("\"{}\"", response.inner()))
            } else {
                Cow::from(&**response.inner())
            };

            //Wrap the content in a JavaScript function call
            format!("{}({});", function, content)
        };

        response.require_header(ContentType(content_type!(Application / Javascript; Charset = Utf8)));
        *response.inner_mut() = output;
    }

    response
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
    fn modify(&self, _ctx: FilterContext, context: &mut Context) -> ContextAction {
        *self.counter.write().unwrap() += 1;
        debug!("Request #{} is to '{}'", *self.counter.read().unwrap(), context.uri_path);
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
        let new_path = context.uri_path.as_path().map(|path| {
            let mut new_path = MaybeUtf8Owned::from("/");
            new_path.push_str(self.prefix.trim_matches('/'));
            new_path.push_bytes(path.as_ref());
            UriPath::Path(new_path)
        });
        if let Some(path) = new_path {
            context.uri_path = path;
        }
        ContextAction::next()
    }
}
