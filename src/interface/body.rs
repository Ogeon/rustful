#[cfg(feature = "rustc_json_body")]
use rustc_serialize::json;
#[cfg(feature = "rustc_json_body")]
use rustc_serialize::Decodable;

#[cfg(feature = "multipart")]
use multipart::server::{HttpRequest, Multipart};

use std::io::{self, Read};
use std::sync::mpsc::{self, Receiver};
use std::cmp::min;

use utils::MAX_BUFFER_LENGTH;

//use hyper::buffer::BufReader;
//use hyper::http::h1::HttpReader;
//use hyper::net::NetworkStream;

use context::Parameters;
use header::Headers;
use handler::Decoder;
use server::Worker;

pub fn new<'a, 'env>(on_body: &'a mut Option<Box<FnMut(&mut Decoder) + Send + 'env>>, worker: Worker<'env>, headers: &Headers) -> Body<'a, 'env> {
    Body::new(on_body, worker, headers)
}

///Provides different read methods for the request body.
pub struct Body<'a, 'env: 'a> {
    on_body: &'a mut Option<Box<FnMut(&mut Decoder) + Send + 'env>>,
    worker: Worker<'env>,

    #[cfg(feature = "multipart")]
    boundary: Option<String>
}

impl<'a, 'env> Body<'a, 'env> {
    #[cfg(feature = "multipart")]
    fn new(on_body: &'a mut Option<Box<FnMut(&mut Decoder) + Send + 'env>>, worker: Worker<'env>, headers: &Headers) -> Body<'a, 'env> {
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

        Body {
            on_body: on_body,
            worker: worker,
            boundary: boundary,
        }
    }

    #[cfg(not(feature = "multipart"))]
    fn new(on_body: &'a mut Option<Box<FnMut(&mut Decoder) + Send + 'env>>, worker: Worker<'env>, _headers: &Headers) -> Body<'a, 'env> {
        Body {
            on_body: on_body,
            worker: worker,
        }
    }

    ///Read the body in a synchronous manner, from a different thread.
    pub fn sync_read<F: FnOnce(BodyReader) + Send + 'env>(mut self, read_fn: F) {
        let (send, recv) = mpsc::channel();
        let reader = BodyReader::new(recv, &mut self);
        self.worker.new_task(move || read_fn(reader));
        self.on_readable(move |decoder| {
            let mut buffer = vec![0; MAX_BUFFER_LENGTH];
            let mut start = 0;
            let mut done = false;

            while start < buffer.len() {
                match decoder.read(&mut buffer[start..]) {
                    Ok(0) => {
                        done = true;
                        break
                    },
                    Ok(length) => start += length,
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e) => {
                        if start > 0 {
                            buffer.truncate(start);
                            send.send(Ok(buffer)).expect("failed to send body part");
                        }
                        send.send(Err(e)).expect("failed to send body read error");
                        return;
                    }
                }
            }

            buffer.truncate(start);
            send.send(Ok(buffer)).expect("failed to send body part");
            if done {
                send.send(Ok(vec![])).expect("failed to send body part");
            }
        });
    }

    ///Read the body as it is, when it becomes readable.
    pub fn on_readable<F: FnMut(&mut Decoder) + Send + 'env>(mut self, on_readable: F) {
        *self.on_body = Some(Box::new(on_readable));
    }

    ///Read the body, and use it when everything has arrived.
    pub fn read_to_end<F: FnOnce(io::Result<Vec<u8>>) + Send + 'env>(self, on_body: F) {
        let mut buffer: Vec<_> = vec![];
        let mut on_body = Some(on_body);
        self.on_readable(move |decoder| {
            let mut start = buffer.len();
            buffer.resize(start + MAX_BUFFER_LENGTH, 0);

            while start < buffer.len() {
                match decoder.read(&mut buffer[start..]) {
                    Ok(0) => {
                        let mut buffer = ::std::mem::replace(&mut buffer, vec![]);
                        buffer.truncate(start);
                        if let Some(on_body) = on_body.take() {
                            on_body(Ok(buffer));
                        }
                        return;
                    },
                    Ok(length) => start += length,
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e) => {
                        if let Some(on_body) = on_body.take() {
                            on_body(Err(e));
                        }
                        break;
                    }
                }
            }

            buffer.truncate(start);
        });
    }

    ///Read and parse the request body as a query string. The body will be
    ///decoded as UTF-8 and plain '+' characters will be replaced with spaces.
    ///
    ///A simplified example of how to parse `a=number&b=number`:
    ///
    ///```
    ///use rustful::{Context, Response};
    ///
    ///fn my_handler<'a, 'env>(context: Context<'a, 'env>, response: Response<'env>) {
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
    pub fn read_query_body<F: FnOnce(io::Result<Parameters>) + Send + 'env>(self, on_body: F) {
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
    ///fn my_handler<'a, 'env>(context: Context<'a, 'env>, response: Response<'env>) {
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
    pub fn read_json_body<F: FnOnce(Result<json::Json, json::BuilderError>) + Send + 'env>(self, on_body: F) {
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
    ///fn my_handler<'a, 'env>(context: Context<'a, 'env>, response: Response<'env>) {
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
    pub fn decode_json_body<T: Decodable, F: FnOnce(Result<T, json::DecoderError>) + Send + 'env>(self, on_body: F) {
        self.read_to_end(move |body| on_body(match body {
            Ok(body) => json::decode(&String::from_utf8_lossy(&body)),
            Err(e) => Err(json::ParserError::from(e).into()),
        }))
    }
}

///A synchronous body reader.
pub struct BodyReader {
    buffer: Option<Vec<u8>>,
    read_pos: usize,
    recv: Receiver<io::Result<Vec<u8>>>,

    #[cfg(feature = "multipart")]
    boundary: Option<String>,
}

impl BodyReader {
    #[cfg(feature = "multipart")]
    fn new(recv: Receiver<io::Result<Vec<u8>>>, body: &mut Body) -> BodyReader {
        BodyReader {
            buffer: None,
            read_pos: 0,
            recv: recv,
            boundary: body.boundary.take(),
        }
    }

    #[cfg(not(feature = "multipart"))]
    fn new(recv: Receiver<io::Result<Vec<u8>>>, _body: &mut Body) -> BodyReader {
        BodyReader {
            buffer: None,
            read_pos: 0,
            recv: recv,
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
    ///fn my_handler<'a, 'env>(context: Context<'a, 'env>, mut response: Response<'env>) {
    ///    context.body.sync_read(move |reader| {
    ///        if let Ok(mut multipart) = reader.into_multipart() {
    ///            let mut result = String::new();
    ///    
    ///            //Iterate over the multipart entries and print info about them in `result`
    ///            multipart.foreach_entry(|entry| match entry.data {
    ///                MultipartData::Text(text) => {
    ///                    //Found data from a text field
    ///                    writeln!(&mut result, "{}: '{}'", entry.name, text);
    ///                },
    ///                MultipartData::File(file) => {
    ///                    //Found an uploaded file
    ///                    if let Some(file_name) = file.filename() {
    ///                        writeln!(&mut result, "{}: a file called '{}'", entry.name, file_name);
    ///                    } else {
    ///                        writeln!(&mut result, "{}: a nameless file", entry.name);
    ///                    }
    ///                }
    ///            });
    ///    
    ///            response.send(result);
    ///        } else {
    ///            //We expected it to be a valid `multipart/form-data` request, but it was not
    ///            response.status = BadRequest;
    ///        }
    ///    });
    ///}
    ///# fn main() {}
    ///```
    #[cfg(feature = "multipart")]
    pub fn into_multipart(self) -> Result<Multipart<BodyReader>, BodyReader> {
        Multipart::from_request(self)
    }
}

impl Read for BodyReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.buffer.is_none() {
            self.buffer = match self.recv.recv() {
                Ok(Ok(v)) => Some(v),
                Ok(Err(e)) => return Err(e),
                Err(_) => None,
            };
            self.read_pos = 0;
        }

        let (res, reset) = if let Some(ref src) = self.buffer {
            let read_len = min(src.len() - self.read_pos, buffer.len());
            buffer[..read_len].clone_from_slice(&src[self.read_pos..self.read_pos + read_len]);
            self.read_pos += read_len;
            (Ok(read_len), self.read_pos == src.len())
        } else {
            (Ok(0), false) //The channel has disconnected. Nothing more to read.
        };

        if reset {
            self.buffer = None;
        }

        res
    }
}

#[cfg(feature = "multipart")]
impl HttpRequest for BodyReader {
    type Body = Self;

    fn body(self) -> Self {
        self
    }

    fn multipart_boundary(&self) -> Option<&str> {
        self.boundary.as_ref().map(AsRef::as_ref)
    }
}
