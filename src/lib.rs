#![crate_id = "rustful#0.1-pre"]

#![comment = "RESTful web framework"]
#![license = "MIT"]
#![crate_type = "rlib"]

#![doc(html_root_url = "http://ogeon.github.io/rustful/doc/")]

#[cfg(test)]
extern crate test;

extern crate collections;
extern crate url;
extern crate time;
extern crate sync;
extern crate http;

pub use router::Router;
pub use server::Server;
pub use request::Request;
pub use response::Response;

pub mod router;
pub mod server;
pub mod request;
pub mod response;