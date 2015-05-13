#![crate_name = "rustful"]

#![crate_type = "rlib"]

#![doc(html_root_url = "http://ogeon.github.io/docs/rustful/master/")]

#![cfg_attr(feature = "nightly", feature(fs_time, path_ext, std_misc))]
#![cfg_attr(test, cfg_attr(feature = "nightly", feature(test)))]

#[cfg(test)]
#[cfg(feature = "nightly")]
extern crate test;

#[cfg(test)]
extern crate tempdir;

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
pub mod context;
pub mod response;
pub mod filter;
pub mod log;

#[cfg(feature = "nightly")]
pub mod cache;

use std::path::Path;
use std::net::{SocketAddr, SocketAddrV4, SocketAddrV6, Ipv4Addr};
use std::str::FromStr;
use std::any::Any;

///HTTP or HTTPS.
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

///A host address and a port.
///
///Can be conveniently converted from an existing address-port pair or just a port:
///
///```
///use std::net::Ipv4Addr;
///use rustful::Host;
///
///let host1: Host = (Ipv4Addr::new(0, 0, 0, 0), 80).into();
///let host2: Host = 80.into();
///
///assert_eq!(host1, host2);
///```
#[derive(Eq, PartialEq, Debug, Hash, Clone, Copy)]
pub struct Host(SocketAddr);

impl Host {
    ///Create a `Host` with the address `0.0.0.0:port`. This is the same as `port.into()`.
    pub fn any_v4(port: u16) -> Host {
        Host(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), port)))
    }

    ///Change the port of the host address.
    pub fn port(&mut self, port: u16) {
        self.0 = match self.0 {
            SocketAddr::V4(addr) => SocketAddr::V4(SocketAddrV4::new(addr.ip().clone(), port)),
            SocketAddr::V6(addr) => {
                SocketAddr::V6(SocketAddrV6::new(addr.ip().clone(), port, addr.flowinfo(), addr.scope_id()))
            }
        };
    }
}

impl From<Host> for SocketAddr {
    fn from(host: Host) -> SocketAddr {
        host.0
    }
}

impl From<u16> for Host {
    fn from(port: u16) -> Host {
        Host::any_v4(port)
    }
}

impl From<SocketAddr> for Host {
    fn from(addr: SocketAddr) -> Host {
        Host(addr)
    }
}

impl From<SocketAddrV4> for Host {
    fn from(addr: SocketAddrV4) -> Host {
        Host(SocketAddr::V4(addr))
    }
}

impl From<SocketAddrV6> for Host {
    fn from(addr: SocketAddrV6) -> Host {
        Host(SocketAddr::V6(addr))
    }
}

impl From<(Ipv4Addr, u16)> for Host {
    fn from((ip, port): (Ipv4Addr, u16)) -> Host {
        Host(SocketAddr::V4(SocketAddrV4::new(ip, port)))
    }
}

impl FromStr for Host {
    type Err = <SocketAddr as FromStr>::Err;

    fn from_str(s: &str) -> Result<Host, Self::Err> {
        s.parse().map(|s| Host(s))
    }
}

///Globally accessible data.
///
///This structure can currently only hold either one item or nothing.
///A future version will be able to hold multiple items.
pub enum Global {
    None,
    One(Box<Any + Send + Sync>),
}

impl Global {
    ///Borrow a value of type `T` if the there is one.
    pub fn get<T: Any + Send + Sync>(&self) -> Option<&T> {
        match self {
            &Global::None => None,
            &Global::One(ref a) => (a as &Any).downcast_ref(),
        }
    }
}

impl<T: Any + Send + Sync> From<Box<T>> for Global {
    fn from(data: Box<T>) -> Global {
        Global::One(data)
    }
}

impl Default for Global {
    fn default() -> Global {
        Global::None
    }
}
