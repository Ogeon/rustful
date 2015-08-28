//!A light HTTP framework with REST-like features. The main purpose of Rustful
//!is to create a simple, modular and non-intrusive foundation for HTTP
//!applications. It has a mainly stateless structure, which naturally allows
//!it to run both as one single server and as multiple instances in a cluster.
//!
//!A new server is created using the [`Server`][server] type, which contains
//!all the necessary settings as fields:
//!
//!```no_run
//!#[macro_use]
//!extern crate rustful;
//!use rustful::{Server, Handler, Context, Response, TreeRouter};
//!
//!struct Greeting(&'static str);
//!
//!impl Handler for Greeting {
//!    fn handle_request(&self, context: Context, response: Response) {
//!        //Check if the client accessed /hello/:name or /good_bye/:name
//!        if let Some(name) = context.variables.get("name") {
//!            //Use the value of :name
//!            response.send(format!("{}, {}", self.0, name));
//!        } else {
//!            response.send(self.0)
//!        }
//!    }
//!}
//!
//!# fn main() {
//!let my_router = insert_routes!{
//!    //Create a new TreeRouter
//!    TreeRouter::new() => {
//!        //Receive GET requests to /hello and /hello/:name
//!        "hello" => {
//!            Get: Greeting("hello"),
//!            ":name" => Get: Greeting("hello")
//!        },
//!        //Receive GET requests to /good_bye and /good_bye/:name
//!        "good_bye" => {
//!            Get: Greeting("good bye"),
//!            ":name" => Get: Greeting("good bye")
//!        }
//!    }
//!};
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

#[cfg(test)]
extern crate tempdir;

#[cfg(feature = "rustc-serialize")]
extern crate rustc_serialize;

#[cfg(feature = "multipart")]
extern crate multipart;

extern crate url;
extern crate time;
extern crate hyper;
extern crate anymap;
extern crate phf;

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
pub use self::handler::Handler;
pub use self::router::Router;
pub use self::log::Log;
pub use self::router::TreeRouter;

mod utils;
#[macro_use]
#[doc(hidden)]
pub mod macros;

pub mod server;
pub mod router;
pub mod handler;
pub mod context;
pub mod response;
pub mod filter;
pub mod log;
pub mod file;
