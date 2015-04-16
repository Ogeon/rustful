#![crate_name = "rustful"]

#![crate_type = "rlib"]

#![doc(html_root_url = "http://ogeon.github.io/rustful/doc/")]

#![feature(fs_time, path_ext, std_misc, core)]
#![cfg_attr(test, feature(test))]

#[cfg(test)] extern crate test;
#[cfg(test)] extern crate tempdir;

extern crate url;
extern crate time;
extern crate hyper;
extern crate anymap;

pub use hyper::mime;
pub use hyper::method::Method;
pub use hyper::status::StatusCode;
pub use hyper::header;
pub use hyper::HttpResult;
pub use hyper::HttpError;
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
#[macro_use] mod macros;

pub mod server;
pub mod router;
pub mod handler;
pub mod cache;
pub mod context;
pub mod response;
pub mod plugin;
pub mod log;

use std::path::Path;

///HTTP or HTTPS
pub enum Scheme<'a> {
    ///Standard HTTP.
    Http,

    ///HTTP with SSL encryption.
    Https {
        ///Path to SSL certificate.
        cert: &'a Path,

        ///Path to key file.
        key: &'a Path
    }
}