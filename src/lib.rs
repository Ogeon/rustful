#[crate_id = "rustful#0.1-pre"];

#[comment = "RESTful web framework"];
#[license = "MIT"];
#[crate_type = "rlib"];

extern crate extra;
extern crate http;

pub use router::Router;
pub use server::Server;
pub use request::Request;

pub mod router;
pub mod server;
pub mod request;