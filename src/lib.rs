#![crate_name = "rustful"]

#![crate_type = "rlib"]

#![doc(html_root_url = "http://ogeon.github.io/rustful/doc/")]

#![feature(unsafe_destructor, old_impl_check, io, collections, std_misc, core)]

#![stable]

#[cfg(test)]
extern crate test;

extern crate url;
extern crate time;
extern crate hyper;

pub use hyper::mime;
pub use hyper::method::Method;
pub use hyper::status::StatusCode;
pub use hyper::header;
pub use hyper::HttpResult;
pub use hyper::HttpError;

pub use self::server::Server;
pub use self::context::Context;
pub use self::response::Response;
pub use self::response::ResponseError;
pub use self::handler::Handler;
pub use self::router::Router;
pub use self::cache::Cache;
pub use self::log::Log;
pub use self::router::TreeRouter;

mod utils;

pub mod server;
pub mod router;
pub mod handler;
pub mod cache;
pub mod context;
pub mod response;
pub mod plugin;
pub mod log;
pub mod macros;

///HTTP or HTTPS
#[stable]
pub enum Protocol {
    ///Standard HTTP.
    #[stable]
    Http,

    ///HTTP with SSL encryption.
    #[stable]
    Https {
        ///Path to SSL certificate.
        cert: Path,

        ///Path to key file.
        key: Path
    }
}