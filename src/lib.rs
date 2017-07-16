//!A light HTTP framework with REST-like features. The main purpose of Rustful
//!is to create a simple, modular and non-intrusive foundation for HTTP
//!applications. It has a mainly stateless structure, which naturally allows
//!it to run both as one single server and as multiple instances in a cluster.
//!
//!A new server is created using the [`Server`][server] type, which contains
//!all the necessary settings as fields:
//!
//!```no_run
//!extern crate rustful;
//!use std::borrow::Cow;
//!use rustful::{Server, CreateContent, Context, Response, DefaultRouter};
//!
//!struct Phrase(&'static str);
//!
//!impl CreateContent for Phrase {
//!    type Output = Cow<'static, str>;
//!
//!    fn create_content(&self, context: &mut Context, _: &Response) -> Cow<'static, str> {
//!        //Check if the client accessed /hello/:name or /good_bye/:name
//!        if let Some(name) = context.variables.get("name") {
//!            //Use the value of :name
//!            format!("{}, {}", self.0, name).into()
//!        } else {
//!            self.0.into()
//!        }
//!    }
//!}
//!
//!# fn main() {
//!let mut my_router = DefaultRouter::<Phrase>::new();
//!my_router.build().many(|mut node| {
//!    //Receive GET requests to /hello and /hello/:name
//!    node.path("hello").many(|mut node| {
//!        node.then().on_get(Phrase("hello"));
//!        node.path(":name").then().on_get(Phrase("hello"));
//!    });
//!
//!    //Receive GET requests to /good_bye and /good_bye/:name
//!    node.path("good_bye").many(|mut node| {
//!        node.then().on_get(Phrase("good bye"));
//!        node.path(":name").then().on_get(Phrase("good bye"));
//!    });
//!});
//!
//!Server {
//!    //Use a closure to handle requests.
//!    handlers: my_router,
//!    //Set the listening port to `8080`.
//!    host: 8080.into(),
//!    //Fill out everything else with default values.
//!    ..Server::default()
//!}.run();
//!# }
//!```
//!
//![server]: server/struct.Server.html

#![crate_name = "rustful"]

#![crate_type = "rlib"]

#![doc(html_root_url = "http://ogeon.github.io/docs/rustful/master/")]

#![cfg_attr(all(test, feature = "benchmark"), feature(test))]
#![cfg_attr(feature = "strict", deny(missing_docs))]
#![cfg_attr(feature = "strict", deny(warnings))]

#[cfg(test)]
#[cfg(feature = "benchmark")]
extern crate test;

#[cfg(feature = "rustc-serialize")]
extern crate rustc_serialize;

#[cfg(feature = "multipart")]
extern crate multipart;

extern crate url;
extern crate time;
extern crate hyper;
extern crate anymap;
extern crate phf;
extern crate num_cpus;
#[macro_use]
extern crate log;

pub use hyper::mime;
pub use hyper::method::Method;
pub use hyper::status::StatusCode;
pub use hyper::header;
pub use hyper::Result as HttpResult;
pub use hyper::Error as HttpError;
pub use hyper::version::HttpVersion;

pub use self::server::Server;
pub use self::context::Context;
pub use self::response::Response;
pub use self::response::Error;
pub use self::response::SendResponse;
pub use self::response::ResponseParams;
pub use self::handler::DefaultRouter;
pub use self::handler::CreateContent;
pub use self::handler::OrElse;
pub use self::handler::StatusRouter;

mod utils;
#[macro_use]
#[doc(hidden)]
pub mod macros;

pub mod server;
pub mod handler;
pub mod context;
pub mod response;
pub mod filter;
pub mod file;
pub mod net;
