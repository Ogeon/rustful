//!Anything related to reading the request body.

#[cfg(feature = "rustc_json_body")]
use rustc_serialize::json;
#[cfg(feature = "rustc_json_body")]
use rustc_serialize::Decodable;

#[cfg(feature = "multipart")]
use multipart::server::{HttpRequest, Multipart};

use std::io::{self, Read};

use utils::FnBox;

//use hyper::buffer::BufReader;
//use hyper::http::h1::HttpReader;
//use hyper::net::NetworkStream;

use context::Parameters;
use header::Headers;

///A reader for a request body.
pub struct BodyReader<'a> {
    on_body: &'a mut Option<Box<FnBox<io::Result<Vec<u8>>, Output=()> + Send>>,

    //#[cfg(feature = "multipart")]
    //multipart_boundary: Option<String>
}

impl<'a> BodyReader<'a> {
    #[doc(hidden)]
    pub fn new(on_body: &'a mut Option<Box<FnBox<io::Result<Vec<u8>>, Output=()> + Send>>) -> BodyReader<'a> {
        BodyReader {
            on_body: on_body,
        }
    }

    ///Read the body, and use it when everything has arrived.
    pub fn read_to_end<F: FnOnce(io::Result<Vec<u8>>) + Send + 'static>(mut self, on_body: F) {
        *self.on_body = Some(Box::new(on_body) as Box<FnBox<io::Result<Vec<u8>>, Output=()> + Send>);
    }

    ///Read and parse the request body as a query string. The body will be
    ///decoded as UTF-8 and plain '+' characters will be replaced with spaces.
    ///
    ///A simplified example of how to parse `a=number&b=number`:
    ///
    ///```
    ///use rustful::{Context, Response};
    ///
    ///fn my_handler(context: Context, response: Response) {
    ///    //Parse the request body as a query string
    ///    context.body.read_query_body(move |query| {
    ///        let query = query.expect("failed to decode query body");
    ///
    ///        //Find "a" and "b" and assume that they are numbers
    ///        let a: f64 = query.get("a").and_then(|number| number.parse().ok()).unwrap();
    ///        let b: f64 = query.get("b").and_then(|number| number.parse().ok()).unwrap();
    ///
    ///        response.send(format!("{} + {} = {}", a, b, a + b));
    ///    });
    ///}
    ///```
    #[inline]
    pub fn read_query_body<F: FnOnce(io::Result<Parameters>) + Send + 'static>(self, on_body: F) {
        self.read_to_end(move |body| on_body(body.map(|buf| ::utils::parse_parameters(&buf))))
    }

    ///Read the request body into a generic JSON structure. This structure can
    ///then be navigated and parsed freely.
    ///
    ///A simplified example of how to parse `{ "a": number, "b": number }`:
    ///
    ///```
    ///use rustful::{Context, Response};
    ///
    ///fn my_handler(context: Context, response: Response) {
    ///    //Parse the request body as JSON
    ///    context.body.read_json_body(move |json| {
    ///        let json = json.expect("failed to decode josn");
    ///
    ///        //Find "a" and "b" in the root object and assume that they are numbers
    ///        let a = json.find("a").and_then(|number| number.as_f64()).unwrap();
    ///        let b = json.find("b").and_then(|number| number.as_f64()).unwrap();
    ///
    ///        response.send(format!("{} + {} = {}", a, b, a + b));
    ///    });
    ///}
    ///```
    #[cfg(feature = "rustc_json_body")]
    pub fn read_json_body<F: FnOnce(Result<json::Json, json::BuilderError>) + Send + 'static>(self, on_body: F) {
        self.read_to_end(move |body| on_body(match body {
            Ok(body) => json::Json::from_str(&String::from_utf8_lossy(&body)),
            Err(e) => Err(e.into()),
        }))
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
    ///fn my_handler(context: Context, response: Response) {
    ///    //Decode a JSON formatted request body into Foo
    ///    context.body.decode_json_body(move |foo| {
    ///        let foo: Foo = foo.expect("failed to decode 'Foo'");
    ///
    ///        response.send(format!("{} + {} = {}", foo.a, foo.b, foo.a + foo.b));
    ///    });
    ///}
    ///# fn main() {}
    ///```
    #[cfg(feature = "rustc_json_body")]
    pub fn decode_json_body<T: Decodable, F: FnOnce(Result<T, json::DecoderError>) + Send + 'static>(self, on_body: F) {
        self.read_to_end(move |body| on_body(match body {
            Ok(body) => json::decode(&String::from_utf8_lossy(&body)),
            Err(e) => Err(json::ParserError::from(e).into()),
        }))
    }
}

/*impl<'a, 'b> BodyReader<'a, 'b> {
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
}*/
