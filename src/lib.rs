#![crate_name = "rustful"]

#![crate_type = "rlib"]

#![doc(html_root_url = "http://ogeon.github.io/rustful/doc/")]

#![feature(unsafe_destructor, fs_time, path_ext, collections, std_misc, str_char)]
#![cfg_attr(test, feature(test))]

#![stable]

#[cfg(test)] extern crate test;
#[cfg(test)] extern crate tempdir;

extern crate url;
extern crate time;
extern crate hyper;

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
pub use self::response::ResponseError;
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
#[stable]
pub enum Scheme<'a> {
    ///Standard HTTP.
    #[stable]
    Http,

    ///HTTP with SSL encryption.
    #[stable]
    Https {
        ///Path to SSL certificate.
        cert: &'a Path,

        ///Path to key file.
        key: &'a Path
    }
}