#[crate_id = "rustful#0.1-pre"];

#[comment = "RESTful web framework"];
#[license = "MIT"];
#[crate_type = "lib"];
#[crate_type = "rlib"];

#[cfg(test)]
extern crate test;

extern crate extra;
extern crate time;
extern crate http;

pub use router::Router;
pub use server::Server;
pub use request::Request;
pub use response::Response;

pub mod router;
pub mod server;
pub mod request;
pub mod response;