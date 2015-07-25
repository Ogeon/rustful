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
//![body_reader]: struct.BodyReader.html

use std::collections::HashMap;
use std::io::{self, Read};
use std::net::SocketAddr;

#[cfg(feature = "rustc_json_body")]
use rustc_serialize::json;
#[cfg(feature = "rustc_json_body")]
use rustc_serialize::Decodable;

use hyper::http::h1::HttpReader;
use hyper::net::NetworkStream;
use hyper::buffer::BufReader;

#[cfg(feature = "multipart")]
use multipart::server::{HttpRequest, Multipart};

use utils;

use HttpVersion;
use Method;
use header::Headers;
use log::Log;

use Global;

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

    ///The requested path.
    pub path: String,

    ///Hypermedia from the current endpoint.
    pub hypermedia: Hypermedia<'s>,

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
    pub body: BodyReader<'a, 'b>,
}

///A reader for a request body.
pub struct BodyReader<'a, 'b: 'a> {
    reader: HttpReader<&'a mut BufReader<&'b mut NetworkStream>>,

    #[cfg(feature = "multipart")]
    multipart_boundary: Option<String>
}

#[cfg(feature = "multipart")]
impl<'a, 'b> BodyReader<'a, 'b> {
    pub fn from_reader(reader: HttpReader<&'a mut BufReader<&'b mut NetworkStream>>, headers: &Headers) -> BodyReader<'a, 'b> {
        use header::ContentType;
        use mime::{Mime, TopLevel, SubLevel, Attr, Value};

        let boundary = match headers.get() {
            Some(&ContentType(Mime(TopLevel::Multipart, SubLevel::FormData, ref attrs))) => {
                attrs.iter()
                    .find(|&&(ref attr, _)| attr == &Attr::Boundary)
                    .and_then(|&(_, ref val)| if let Value::Ext(ref boundary) = *val {
                        Some(boundary.clone())
                    } else {
                        None
                    })
            },
            _ => None
        };

        BodyReader {
            reader: reader,
            multipart_boundary: boundary
        }
    }

    ///Try to create a `multipart/form-data` reader from the request body.
    ///
    ///```
    ///# extern crate rustful;
    ///# extern crate multipart;
    ///use std::fmt::Write;
    ///use rustful::{Context, Response};
    ///use rustful::StatusCode::BadRequest;
    ///use multipart::server::MultipartData;
    ///
    ///fn my_handler(mut context: Context, mut response: Response) {
    ///    if let Some(mut multipart) = context.body.as_multipart() {
    ///        let mut result = String::new();
    ///
    ///        //Iterate over the multipart entries and print info about them in `result`
    ///        multipart.foreach_entry(|entry| match entry.data {
    ///            MultipartData::Text(text) => {
    ///                //Found data from a text field
    ///                writeln!(&mut result, "{}: '{}'", entry.name, text);
    ///            },
    ///            MultipartData::File(file) => {
    ///                //Found an uploaded file
    ///                if let Some(file_name) = file.filename() {
    ///                    writeln!(&mut result, "{}: a file called '{}'", entry.name, file_name);
    ///                } else {
    ///                    writeln!(&mut result, "{}: a nameless file", entry.name);
    ///                }
    ///            }
    ///        });
    ///
    ///        response.send(result);
    ///    } else {
    ///        //We expected it to be a valid `multipart/form-data` request, but it was not
    ///        response.set_status(BadRequest);
    ///    }
    ///}
    ///# fn main() {}
    ///```
    pub fn as_multipart<'r>(&'r mut self) -> Option<Multipart<MultipartRequest<'r, 'a, 'b>>> {
        let reader = &mut self.reader;
        self.multipart_boundary.as_ref().and_then(move |boundary|
            Multipart::from_request(MultipartRequest {
                boundary: boundary,
                reader: reader
            }).ok()
        )
    }
}

#[cfg(not(feature = "multipart"))]
impl<'a, 'b> BodyReader<'a, 'b> {
    pub fn from_reader(reader: HttpReader<&'a mut BufReader<&'b mut NetworkStream>>, _headers: &Headers) -> BodyReader<'a, 'b> {
        BodyReader {
            reader: reader
        }
    }
}

///`BodyReader` extension for reading and parsing a query string.
///
///Examples and more information can be found in [the documentation for
///`BodyReader`][body_reader].
///
///[body_reader]: struct.BodyReader.html
pub trait ExtQueryBody {
    fn read_query_body(&mut self) -> io::Result<HashMap<String, String>>;
}

impl<'a, 'b> ExtQueryBody for BodyReader<'a, 'b> {
    ///Read and parse the request body as a query string. The body will be
    ///decoded as UTF-8 and plain '+' characters will be replaced with spaces.
    ///
    ///A simplified example of how to parse `a=number&b=number`:
    ///
    ///```
    ///use rustful::{Context, Response};
    ///use rustful::context::ExtQueryBody;
    ///
    ///fn my_handler(mut context: Context, response: Response) {
    ///    //Parse the request body as a query string
    ///    let query = context.body.read_query_body().unwrap();
    ///
    ///    //Find "a" and "b" and assume that they are numbers
    ///    let a: f64 = query.get("a").and_then(|number| number.parse().ok()).unwrap();
    ///    let b: f64 = query.get("b").and_then(|number| number.parse().ok()).unwrap();
    ///
    ///    response.send(format!("{} + {} = {}", a, b, a + b));
    ///}
    ///```
    #[inline]
    fn read_query_body(&mut self) -> io::Result<HashMap<String, String>> {
        let mut buf = Vec::new();
        try!(self.read_to_end(&mut buf));
        Ok(utils::parse_parameters(&buf))
    }
}

///`BodyReader` extension for reading and parsing a JSON body.
///
///It is available by default and can be toggled using the `rustc_json_body`
///feature. Examples and more information can be found in [the documentation
///for `BodyReader`][body_reader].
///
///[body_reader]: struct.BodyReader.html
#[cfg(feature = "rustc_json_body")]
pub trait ExtJsonBody {
    ///Read the request body into a JSON structure.
    fn read_json_body(&mut self) -> Result<json::Json, json::BuilderError>;

    ///Parse and decode the request body as some type `T`.
    fn decode_json_body<T: Decodable>(&mut self) -> json::DecodeResult<T>;
}

#[cfg(feature = "rustc_json_body")]
impl<'a, 'b> ExtJsonBody for BodyReader<'a, 'b> {
    ///Read the request body into a generic JSON structure. This structure can
    ///then be navigated and parsed freely.
    ///
    ///A simplified example of how to parse `{ "a": number, "b": number }`:
    ///
    ///```
    ///use rustful::{Context, Response};
    ///use rustful::context::ExtJsonBody;
    ///
    ///fn my_handler(mut context: Context, response: Response) {
    ///    //Parse the request body as JSON
    ///    let json = context.body.read_json_body().unwrap();
    ///
    ///    //Find "a" and "b" in the root object and assume that they are numbers
    ///    let a = json.find("a").and_then(|number| number.as_f64()).unwrap();
    ///    let b = json.find("b").and_then(|number| number.as_f64()).unwrap();
    ///
    ///    response.send(format!("{} + {} = {}", a, b, a + b));
    ///}
    ///```
    fn read_json_body(&mut self) -> Result<json::Json, json::BuilderError> {
        json::Json::from_reader(self)
    }

    ///Read and decode a request body as a type `T`. The target type must
    ///implement `rustc_serialize::Decodable`.
    ///
    ///A simplified example of how to parse `{ "a": number, "b": number }`:
    ///
    ///```
    ///extern crate rustful;
    ///extern crate rustc_serialize;
    ///
    ///use rustful::{Context, Response};
    ///use rustful::context::ExtJsonBody;
    ///
    ///#[derive(RustcDecodable)]
    ///struct Foo {
    ///    a: f64,
    ///    b: f64
    ///}
    ///
    ///fn my_handler(mut context: Context, response: Response) {
    ///    //Decode a JSON formatted request body into Foo
    ///    let foo: Foo = context.body.decode_json_body().unwrap();
    ///
    ///    response.send(format!("{} + {} = {}", foo.a, foo.b, foo.a + foo.b));
    ///}
    ///# fn main() {}
    ///```
    fn decode_json_body<T: Decodable>(&mut self) -> json::DecodeResult<T> {
        let mut buf = String::new();
        try!(self.read_to_string(&mut buf).map_err(|e| {
            let parse_err = json::ParserError::IoError(e);
            json::DecoderError::ParseError(parse_err)
        }));
        json::decode(&buf)
    }
}

impl<'a, 'b> Read for BodyReader<'a, 'b> {
    ///Read the request body.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buf)
    }
}

///A specialized request representation for the multipart interface.
#[cfg(feature = "multipart")]
pub struct MultipartRequest<'r, 'a: 'r, 'b: 'a> {
    boundary: &'r str,
    reader: &'r mut HttpReader<&'a mut BufReader<&'b mut NetworkStream>>
}

#[cfg(feature = "multipart")]
impl<'r, 'a, 'b> HttpRequest for MultipartRequest<'r, 'a, 'b> {
    fn is_multipart(&self) -> bool {
        true
    }

    fn boundary(&self) -> Option<&str> {
        Some(self.boundary)
    }
}

#[cfg(feature = "multipart")]
impl<'r, 'a, 'b> Read for MultipartRequest<'r, 'a, 'b> {
    ///Read the request body.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buf)
    }
}

///Hypermedia connected to an API endpoint.
pub struct Hypermedia<'a> {
    ///Forward links from the current endpoint to other endpoints. These may
    ///include endpoints with the same path, but different method.
    pub links: Vec<Link<'a>>
}

impl<'a> Hypermedia<'a> {
    ///Create an empty `Hypermedia` structure.
    pub fn new() -> Hypermedia<'a> {
        Hypermedia {
            links: vec![]
        }
    }
}

///A hyperlink.
#[derive(PartialEq, Eq, Debug)]
pub struct Link<'a> {
    ///The HTTP method for which an endpoint is available. It can be left
    ///unspecified if the method doesn't matter.
    pub method: Option<Method>,
    ///A relative path from the current location.
    pub path: Vec<LinkSegment<'a>>
}

///A segment of a hyperlink path.
#[derive(PartialEq, Eq, Debug)]
pub enum LinkSegment<'a> {
    ///A static part of a path.
    Static(&'a str),
    ///A dynamic part of a path. Can be substituted with anything.
    Variable(&'a str),
    ///A recursive wildcard. Will recursively match anything.
    RecursiveWildcard
}
