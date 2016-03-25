//!Anything related to reading the request body.

#[cfg(feature = "rustc_json_body")]
use rustc_serialize::json;
#[cfg(feature = "rustc_json_body")]
use rustc_serialize::Decodable;

#[cfg(feature = "multipart")]
use multipart::server::{HttpRequest, Multipart};

use std::io::{self, Read};

use hyper::buffer::BufReader;
use hyper::http::h1::HttpReader;
use hyper::net::NetworkStream;

use context::Parameters;
use header::Headers;

///A reader for a request body.
pub struct BodyReader<'a, 'b: 'a> {
    reader: HttpReader<&'a mut BufReader<&'b mut NetworkStream>>,

    #[cfg(feature = "multipart")]
    multipart_boundary: Option<String>
}

impl<'a, 'b> BodyReader<'a, 'b> {
    #[doc(hidden)]
    #[cfg(feature = "multipart")]
    ///Internal and may change without warning.
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

    #[doc(hidden)]
    #[cfg(not(feature = "multipart"))]
    ///Internal and may change without warning.
    pub fn from_reader(reader: HttpReader<&'a mut BufReader<&'b mut NetworkStream>>, _headers: &Headers) -> BodyReader<'a, 'b> {
        BodyReader {
            reader: reader
        }
    }
}

impl<'a, 'b> BodyReader<'a, 'b> {
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
    #[cfg(feature = "multipart")]
    pub fn as_multipart<'r>(&'r mut self) -> Option<Multipart<MultipartRequest<'r, 'a, 'b>>> {
        let reader = &mut self.reader;
        self.multipart_boundary.as_ref().and_then(move |boundary|
            Multipart::from_request(MultipartRequest {
                boundary: boundary,
                reader: reader
            }).ok()
        )
    }

    ///Read and parse the request body as a query string. The body will be
    ///decoded as UTF-8 and plain '+' characters will be replaced with spaces.
    ///
    ///A simplified example of how to parse `a=number&b=number`:
    ///
    ///```
    ///use rustful::{Context, Response};
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
    pub fn read_query_body(&mut self) -> io::Result<Parameters> {
        let mut buf = Vec::new();
        try!(self.read_to_end(&mut buf));
        Ok(::utils::parse_parameters(&buf))
    }

    ///Read the request body into a generic JSON structure. This structure can
    ///then be navigated and parsed freely.
    ///
    ///A simplified example of how to parse `{ "a": number, "b": number }`:
    ///
    ///```
    ///use rustful::{Context, Response};
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
    #[cfg(feature = "rustc_json_body")]
    pub fn read_json_body(&mut self) -> Result<json::Json, json::BuilderError> {
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
    #[cfg(feature = "rustc_json_body")]
    pub fn decode_json_body<T: Decodable>(&mut self) -> json::DecodeResult<T> {
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
    type Body = Self;

    fn body(self) -> Self {
        self
    }

    fn multipart_boundary(&self) -> Option<&str> {
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