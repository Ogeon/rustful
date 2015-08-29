//!Handler context and request body reading extensions.
//!
//!#Context
//!
//!The [`Context`][context] contains all the input data for the request
//!handlers, as well as some utilities. This is where request data, like
//!headers, client address and the request body can be retrieved from and it
//!can safely be picked apart, since its ownership is transferred to the
//!handler.
//!
//!##Accessing Headers
//!
//!The headers are stored in the `headers` field. See the [`Headers`][headers]
//!struct for more information about how to access them.
//!
//!```
//!use rustful::{Context, Response};
//!use rustful::header::UserAgent;
//!
//!fn my_handler(context: Context, response: Response) {
//!    if let Some(&UserAgent(ref user_agent)) = context.headers.get() {
//!        response.send(format!("got user agent string \"{}\"", user_agent));
//!    } else {
//!        response.send("no user agent string provided");
//!    }
//!}
//!```
//!
//!##Path Variables
//!
//!A router may collect variable data from paths (for example `id` in
//!`/products/:id`). The values from these variables can be accessed through
//!the `variables` field.
//!
//!```
//!use rustful::{Context, Response};
//!
//!fn my_handler(context: Context, response: Response) {
//!    if let Some(id) = context.variables.get("id") {
//!        response.send(format!("asking for product with id \"{}\"", id));
//!    } else {
//!        //This will usually not happen, unless the handler is also
//!        //assigned to a path without the `id` variable
//!        response.send("no id provided");
//!    }
//!}
//!```
//!
//!##Other URL Parts
//!
//! * Query variables (`http://example.com?a=b&c=d`) can be found in the
//!`query` field and they are accessed in exactly the same fashion as path
//!variables are used.
//!
//! * The fragment (`http://example.com#foo`) is also parsed and can be
//!accessed through `fragment` as an optional `String`.
//!
//!##Logging
//!
//!Rustful has a built in logging infrastructure and it is made available to
//!handlers through the `log` field. This provides logging and error reporting
//!in a unified and more controlled fashion than what panics and `println!`
//!gives. See the [`log`][log] module for more information about the standard
//!alternatives.
//!
//!```
//!# fn something_that_may_fail() -> Result<&'static str, &'static str> { Ok("yo") }
//!use rustful::{Context, Response};
//!use rustful::StatusCode::InternalServerError;
//!
//!fn my_handler(context: Context, mut response: Response) {
//!    match something_that_may_fail() {
//!        Ok(res) => response.send(res),
//!        Err(e) => {
//!            context.log.error(&format!("it failed! {}", e));
//!            response.set_status(InternalServerError);
//!        }
//!    }
//!}
//!```
//!
//!##Global Data
//!
//!There is also infrastructure for globally accessible data, that can be
//!accessed through the `global` field. This is meant to provide a place for
//!things like database connections or cached data that should be available to
//!all handlers. The storage space itself is immutable when the server has
//!started, so the only way to change it is through some kind of inner
//!mutability.
//!
//!```
//!use rustful::{Context, Response};
//!use rustful::StatusCode::InternalServerError;
//!
//!fn my_handler(context: Context, mut response: Response) {
//!    if let Some(some_wise_words) = context.global.get::<&str>() {
//!        response.send(format!("food for thought: {}", some_wise_words));
//!    } else {
//!        context.log.error("there should be a string literal in `global`");
//!        response.set_status(InternalServerError);
//!    }
//!}
//!```
//!
//!##Request Body
//!
//!The body will not be read in advance, unlike the other parts of the
//!request. It is instead available as a `BodyReader` in the field `body`,
//!through which it can be read and parsed as various data formats, like JSON
//!and query strings. The documentation for [`BodyReader`][body_reader] gives
//!more examples.
//!
//!```
//!use std::io::{BufReader, BufRead};
//!use rustful::{Context, Response};
//!
//!fn my_handler(context: Context, response: Response) {
//!    let mut numbered_lines = BufReader::new(context.body).lines().enumerate();
//!    let mut writer = response.into_chunked();
//!
//!    while let Some((line_no, Ok(line))) = numbered_lines.next() {
//!        writer.send(format!("{}: {}", line_no + 1, line));
//!    }
//!}
//!```
//!
//![context]: struct.Context.html
//![headers]: ../header/struct.Headers.html
//![log]: ../log/index.html
//![body_reader]: body/struct.BodyReader.html

use std::net::SocketAddr;
use std::fmt;
use std::borrow::Cow;

use HttpVersion;
use Method;
use header::Headers;
use log::Log;
use server::Global;

use self::body::BodyReader;
use self::hypermedia::Hypermedia;

pub mod body;
pub mod hypermedia;

mod maybe_utf8;
pub use self::maybe_utf8::{MaybeUtf8, MaybeUtf8Owned, MaybeUtf8Slice};

mod parameters;
pub use self::parameters::Parameters;

///A container for handler input, like request data and utilities.
pub struct Context<'a, 'b: 'a, 's> {
    ///Headers from the HTTP request.
    pub headers: Headers,

    ///The HTTP version used in the request.
    pub http_version: HttpVersion,

    ///The client address
    pub address: SocketAddr,

    ///The HTTP method.
    pub method: Method,

    ///The requested URI.
    pub uri: Uri,

    ///Hypermedia from the current endpoint.
    pub hypermedia: Hypermedia<'s>,

    ///Route variables.
    pub variables: Parameters,

    ///Query variables from the path.
    pub query: Parameters,

    ///The fragment part of the URL (after #), if provided.
    pub fragment: Option<MaybeUtf8Owned>,

    ///Log for notes, errors and warnings.
    pub log: &'s (Log + 's),

    ///Globally accessible data.
    pub global: &'s Global,

    ///A reader for the request body.
    pub body: BodyReader<'a, 'b>,
}

///A URI that can be a path or an asterisk (`*`).
///
///The URI may be an invalid UTF-8 path and it is therefore represented as a
///percent decoded byte vector, but can easily be parsed as a string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Uri {
    ///A path URI.
    Path(MaybeUtf8Owned),
    ///An asterisk (`*`) URI.
    Asterisk
}

impl Uri {
    ///Borrow the URI as a raw path.
    pub fn as_path(&self) -> Option<MaybeUtf8Slice> {
        match *self {
            Uri::Path(ref path) => Some(path.as_slice()),
            Uri::Asterisk => None
        }
    }

    ///Borrow the URI as a UTF-8 path, if valid.
    pub fn as_utf8_path(&self) -> Option<&str> {
        match *self {
            Uri::Path(ref path) => path.as_utf8(),
            Uri::Asterisk => None
        }
    }

    ///Borrow the URI as a UTF-8 path, if valid, or convert it to a valid
    ///UTF-8 string.
    pub fn as_utf8_path_lossy<'a>(&'a self) -> Option<Cow<'a, str>> {
        match *self {
            Uri::Path(ref path) => Some(path.as_utf8_lossy()),
            Uri::Asterisk => None
        }
    }

    ///Check if the URI is a path.
    pub fn is_path(&self) -> bool {
        match *self {
            Uri::Path(_) => true,
            Uri::Asterisk => false
        }
    }

    ///Check if the URI is an asterisk (`*`).
    pub fn is_asterisk(&self) -> bool {
        match *self {
            Uri::Path(_) => false,
            Uri::Asterisk => true
        }
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_utf8_path_lossy().unwrap_or_else(|| "*".into()).fmt(f)
    }
}
