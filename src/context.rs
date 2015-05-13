//!Handler context and request body reading extensions.

use std::collections::HashMap;
use std::io::{self, Read};
use std::net::SocketAddr;
use std::ops::{Deref, DerefMut};

use hyper::http::HttpReader;
use hyper::net::NetworkStream;
use hyper::buffer::BufReader;

use utils;

use HttpVersion;
use Method;
use header::Headers;
use log::Log;

use Global;

///A container for things like request data and cache.
///
///A `Context` can be dereferenced to a `BodyReader`, allowing direct access to
///the underlying read methods.
pub struct Context<'a, 'b: 'a, 's> {
    ///Headers from the HTTP request.
    pub headers: Headers,

    ///The HTTP version used in the request.
    pub http_version: HttpVersion,

    ///The client address
    pub address: SocketAddr,

    ///The HTTP method.
    pub method: Method,

    ///The requested path.
    pub path: String,

    ///Route variables.
    pub variables: HashMap<String, String>,

    ///Query variables from the path.
    pub query: HashMap<String, String>,

    ///The fragment part of the URL (after #), if provided.
    pub fragment: Option<String>,

    ///Log for notes, errors and warnings.
    pub log: &'s (Log + 's),

    ///Globally accessible data.
    pub global: &'s Global,

    ///A reader for the request body.
    pub body_reader: BodyReader<'a, 'b>,
}

impl<'a, 'b, 's> Deref for Context<'a, 'b, 's> {
    type Target = BodyReader<'a, 'b>;

    fn deref<'r>(&'r self) -> &'r BodyReader<'a, 'b> {
        &self.body_reader
    }
}

impl<'a, 'b, 's> DerefMut for Context<'a, 'b, 's> {
    fn deref_mut<'r>(&'r mut self) -> &'r mut BodyReader<'a, 'b> {
        &mut self.body_reader
    }
}

///A reader for a request body.
pub struct BodyReader<'a, 'b: 'a> {
    request: HttpReader<&'a mut BufReader<&'b mut NetworkStream>>
}

impl<'a, 'b> BodyReader<'a, 'b> {
    pub fn from_reader(request: HttpReader<&'a mut BufReader<&'b mut NetworkStream>>) -> BodyReader<'a, 'b> {
        BodyReader {
            request: request
        }
    }
}

impl<'a, 'b> Read for BodyReader<'a, 'b> {
    ///Read the request body.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.request.read(buf)
    }
}

///`BodyReader` extension for reading and parsing a query string.
pub trait ExtQueryBody {
    fn read_query_body(&mut self) -> io::Result<HashMap<String, String>>;
}

impl<'a, 'b> ExtQueryBody for BodyReader<'a, 'b> {
    ///Read and parse the request body as a query string.
    ///The body will be decoded as UTF-8 and plain '+' characters will be replaced with spaces.
    #[inline]
    fn read_query_body(&mut self) -> io::Result<HashMap<String, String>> {
        let mut buf = Vec::new();
        try!(self.read_to_end(&mut buf));
        Ok(utils::parse_parameters(&buf))
    }
}