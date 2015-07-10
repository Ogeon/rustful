//!Response writers.
//!
//!The response writers are the output channel from the handlers to the
//!client. These are used to set the response headers, as well as writing the
//!response body. Rustful provides three different types of response writers
//!with different purposes:
//!
//! * [`Response`][res] - It's used to write data with a known, fixed size,
//!that is already stored in some kind of buffer.
//! * [`Chunked`][chu] - A chunked response is a streaming response, where the final
//!size can be unknown.
//! * [`Raw`][raw] - This is also a streaming response, but with a fixed size. It is
//!unsafe to create because of the risk of sending too short responses, but it
//!can be very useful in cases where it's impractical to buffer the data, such as when
//!sending large files.
//!
//!You will always start out with a `Response`, where you can set the status
//!code and all the headers, and then transform it into one of the other
//!types, if necessary. It is usually recommended to stick to `Response` as
//!much as possible, since it has lower HTTP overhead than `Chunked` and has a
//!builtin size check that guarantees that the `content-length` field is
//!correct.
//!
//!|            | No extra overhead | Guaranteed correct `content-length` | Streaming |
//!|------------|-------------------|-------------------------------------|-----------|
//!| `Response` | &check;           | &check;                             | &cross;   |
//!| `Raw`      | &check;           | &cross;                             | &check;   |
//!| `Chunked`  | &cross;           | &check;                             | &check;   |
//!
//![res]: struct.Response.html
//![chu]: struct.Chunked.html
//![raw]: struct.Raw.html

use std;
use std::io::{self, Write};
use std::error;
use std::borrow::Cow;
use std::convert::From;
use std::str::{from_utf8, Utf8Error};
use std::string::{FromUtf8Error};

use hyper;

use anymap::AnyMap;

use StatusCode;

use header::Headers;
use filter::{FilterContext, ResponseFilter};
use filter::ResponseAction as Action;
use log::Log;

use Global;

///The result of a response action.
#[derive(Debug)]
pub enum Error {
    ///A response filter failed.
    Filter(String),

    ///There was an IO error.
    Io(io::Error)
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::Io(err)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            Error::Filter(ref desc) => write!(f, "filter error: {}", desc),
            Error::Io(ref e) => write!(f, "io error: {}", e)
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::Filter(ref desc) => desc,
            Error::Io(ref e) => e.description()
        }
    }

    fn cause(&self) -> Option<&std::error::Error> {
        match *self {
            Error::Filter(_) => None,
            Error::Io(ref e) => Some(e)
        }
    }
}

///A unified representation of response data.
#[derive(Clone)]
pub enum Data<'a> {
    ///Data in byte form.
    Bytes(Cow<'a, [u8]>),

    ///Data in string form.
    String(Cow<'a, str>)
}

impl<'a> Data<'a> {
    ///Borrow the content as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            &Data::Bytes(ref bytes) => bytes,
            &Data::String(ref string) => string.as_bytes(),
        }
    }

    ///Turns the content into a byte vector. Slices are copied.
    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            Data::Bytes(bytes) => bytes.into_owned(),
            Data::String(string) => string.into_owned().into_bytes()
        }
    }

    ///Borrow the content as a UTF-8 string slice, if possible.
    pub fn as_string(&self) -> Result<&str, Utf8Error> {
        match self {
            &Data::Bytes(ref bytes) => from_utf8(bytes),
            &Data::String(ref string) => Ok(string),
        }
    }

    ///Turn the content into a UTF-8 string, if possible. Slices are copied.
    pub fn into_string(self) -> Result<String, FromUtf8Error> {
        match self {
            Data::Bytes(bytes) => String::from_utf8(bytes.into_owned()),
            Data::String(string) => Ok(string.into_owned())
        }
    }
}

impl<'a> Into<Data<'a>> for Vec<u8> {
    fn into(self) -> Data<'a> {
        Data::Bytes(Cow::Owned(self))
    }
}

impl<'a> Into<Data<'a>> for &'a [u8] {
    fn into(self) -> Data<'a> {
        Data::Bytes(Cow::Borrowed(self))
    }
}

impl<'a> Into<Data<'a>> for String {
    fn into(self) -> Data<'a> {
        Data::String(Cow::Owned(self))
    }
}

impl<'a> Into<Data<'a>> for &'a str {
    fn into(self) -> Data<'a> {
        Data::String(Cow::Borrowed(self))
    }
}


///An interface for sending data to the client.
///
///This is where the status code and response headers are set, as well as the
///response body. The body can be directly written through the `Response` if
///its size is known.
pub struct Response<'a, 'b> {
    writer: Option<hyper::server::response::Response<'a>>,
    filters: &'b Vec<Box<ResponseFilter>>,
    log: &'b (Log + 'b),
    global: &'b Global,
    filter_storage: Option<AnyMap>
}

impl<'a, 'b> Response<'a, 'b> {
    pub fn new(
        response: hyper::server::response::Response<'a>,
        filters: &'b Vec<Box<ResponseFilter>>,
        log: &'b Log,
        global: &'b Global
    ) -> Response<'a, 'b> {
        Response {
            writer: Some(response),
            filters: filters,
            log: log,
            global: global,
            filter_storage: Some(AnyMap::new())
        }
    }

    ///Get the current status code.
    pub fn status(&self) -> StatusCode {
        self.writer.as_ref().expect("status accessed after drop").status()
    }

    ///Change the status code. `Ok (200)` is the default.
    pub fn set_status(&mut self, status: StatusCode) {
        if let Some(ref mut writer) = self.writer {
            *writer.status_mut() = status;
        }
    }

    ///Get a reference to the headers.
    pub fn headers(&self) -> &Headers {
        self.writer.as_ref().expect("headers accessed after drop").headers()
    }

    ///Get a mutable reference to the headers.
    pub fn headers_mut(&mut self) -> &mut Headers {
        self.writer.as_mut().expect("headers mutably accessed after drop").headers_mut()
    }

    ///Get a reference to the filter storage.
    pub fn filter_storage(&self) -> &AnyMap {
        self.filter_storage.as_ref().expect("filter storage accessed after drop")
    }

    ///Get a mutable reference to the filter storage. It can be used to
    ///communicate with the response filters.
    pub fn filter_storage_mut(&mut self) -> &mut AnyMap {
        self.filter_storage.as_mut().expect("filter storage mutably accessed after drop")
    }

    ///Send data to the client and finish the response, ignoring eventual
    ///errors. Use `try_send` to get error information.
    ///
    ///```
    ///use rustful::{Context, Response};
    ///
    ///fn my_handler(context: Context, response: Response) {
    ///    response.send("hello");
    ///}
    ///```
    #[allow(unused_must_use)]
    pub fn send<'d, Content: Into<Data<'d>>>(self, content: Content) {
        self.try_send(content);
    }

    ///Try to send data to the client and finish the response. This is the
    ///same as `send`, but errors are not ignored.
    ///
    ///```
    ///use rustful::{Context, Response};
    ///use rustful::response::Error;
    ///
    ///fn my_handler(context: Context, response: Response) {
    ///    if let Err(Error::Filter(e)) = response.try_send("hello") {
    ///        context.log.note(&format!("a filter failed: {}", e));
    ///    }
    ///}
    ///```
    pub fn try_send<'d, Content: Into<Data<'d>>>(mut self, content: Content) -> Result<(), Error> {
        self.send_sized(content)
    }

    fn send_sized<'d, Content: Into<Data<'d>>>(&mut self, content: Content) -> Result<(), Error> {
        let mut writer = self.writer.take().expect("response used after drop");
        let mut filter_storage = self.filter_storage.take().expect("response used after drop");

        if self.filters.is_empty() {
            writer.send(content.into().as_bytes()).map_err(|e| e.into())
        } else {
            let mut buffer = vec![];

            let (status, write_queue) = try!(filter_headers(
                self.filters,
                writer.status(),
                writer.headers_mut(),
                self.log,
                self.global,
                &mut filter_storage
            ));
            *writer.status_mut() = status;
            for action in write_queue {
                match action {
                    Action::Next(Some(content)) => try!(buffer.write_all(content.as_bytes())),
                    Action::Next(None) => {},
                    Action::Abort(e) => return Err(Error::Filter(e)),
                    Action::SilentAbort => break
                }
            }

            let filter_result = filter_content(self.filters, content, self.log, self.global, &mut filter_storage);
            match filter_result {
                Action::Next(Some(content)) => try!(buffer.write_all(content.as_bytes())),
                Action::Abort(e) => return Err(Error::Filter(e)),
                _ => {}
            }

            let write_queue = try!(filter_end(self.filters, self.log, self.global, &mut filter_storage));
            for action in write_queue {
                match action {
                    Action::Next(Some(content)) => try!(buffer.write_all(content.as_bytes())),
                    Action::Next(None) => {},
                    Action::Abort(e) => return Err(Error::Filter(e)),
                    Action::SilentAbort => break
                }
            }
            
            writer.send(&buffer).map_err(|e| e.into())
        }
    }

    ///Write the status code and headers to the client and turn the `Response`
    ///into a `Chunked` response.
    pub fn into_chunked(mut self) -> Chunked<'a, 'b> {
        let mut writer = self.writer.take().expect("response used after drop");
        
        //Make sure it's chunked
        writer.headers_mut().remove::<::header::ContentLength>();
        writer.headers_mut().remove_raw("content-length");

        let writer = filter_headers(
            self.filters,
            writer.status(),
            writer.headers_mut(),
            self.log,
            self.global,
            self.filter_storage_mut()
        ).and_then(|(status, write_queue)|{
            *writer.status_mut() = status;
            let mut writer = try!(writer.start());

            for action in write_queue {
                match action {
                    Action::Next(Some(content)) => try!(writer.write_all(content.as_bytes())),
                    Action::Next(None) => {},
                    Action::Abort(e) => return Err(Error::Filter(e)),
                    Action::SilentAbort => break
                }
            }

            Ok(writer)
        });

        Chunked {
            writer: Some(writer),
            filters: self.filters,
            log: self.log,
            global: self.global,
            filter_storage: self.filter_storage.take().expect("response used after drop")
        }
    }

    ///Write the status code and headers to the client and turn the `Response`
    ///into a `Raw` response. Any eventual response filters are bypassed to
    ///make sure that the data is not modified.
    ///
    ///__Unsafety__: The content length is set beforehand, which makes it
    ///possible to send responses that are too short.
    pub unsafe fn into_raw(mut self, content_length: u64) -> Raw<'a> {
        let mut writer = self.writer.take().expect("response used after drop");

        writer.headers_mut().remove_raw("content-length");
        writer.headers_mut().set(::header::ContentLength(content_length));

        Raw {
            writer: Some(writer.start())
        }
    }
}

#[allow(unused_must_use)]
impl<'a, 'b> Drop for Response<'a, 'b> {
    ///Writes status code and headers and closes the connection.
    fn drop(&mut self) {
        if self.writer.is_some() {
            self.send_sized(&[][..]);
        }
    }
}


///An interface for writing a chunked response body.
///

///This is useful for when the size of the data is unknown, but it comes with
///an overhead for each time `send` or `try_send` is called (simply put).
pub struct Chunked<'a, 'b> {
    writer: Option<Result<hyper::server::response::Response<'a, hyper::net::Streaming>, Error>>,
    filters: &'b Vec<Box<ResponseFilter>>,
    log: &'b (Log + 'b),
    global: &'b Global,
    filter_storage: AnyMap
}

impl<'a, 'b> Chunked<'a, 'b> {
    ///Get a reference to the filter storage.
    pub fn filter_storage(&self) -> &AnyMap {
        &self.filter_storage
    }

    ///Get a mutable reference to the filter storage. It can be used to
    ///communicate with the response filters.
    pub fn filter_storage_mut(&mut self) -> &mut AnyMap {
        &mut self.filter_storage
    }

    ///Send a chunk of data to the client, ignoring any eventual errors. Use
    ///`try_send` to get error information.
    ///
    ///```
    ///use rustful::{Context, Response};
    ///
    ///fn my_handler(context: Context, response: Response) {
    ///    let count = context.variables.get("count")
    ///        .and_then(|n| n.parse().ok())
    ///        .unwrap_or(0u32);
    ///    let mut chunked = response.into_chunked();
    ///
    ///    for i in 0..count {
    ///        chunked.send(format!("chunk #{}", i + 1));
    ///    }
    ///}
    ///```
    #[allow(unused_must_use)]
    pub fn send<'d, Content: Into<Data<'d>>>(&mut self, content: Content) {
        self.try_send(content);
    }

    ///Send a chunk of data to the client. This is the same as `send`, but
    ///errors are not ignored.
    ///
    ///```
    ///use rustful::{Context, Response};
    ///use rustful::response::Error;
    ///
    ///fn my_handler(context: Context, response: Response) {
    ///    let count = context.variables.get("count")
    ///        .and_then(|n| n.parse().ok())
    ///        .unwrap_or(0u32);
    ///    let mut chunked = response.into_chunked();
    ///
    ///    for i in 0..count {
    ///        if let Err(Error::Filter(e)) = chunked.try_send(format!("chunk #{}", i + 1)) {
    ///            context.log.note(&format!("a filter failed: {}", e));
    ///        }
    ///    }
    ///}
    ///```
    pub fn try_send<'d, Content: Into<Data<'d>>>(&mut self, content: Content) -> Result<usize, Error> {
        let mut writer = match self.writer {
            Some(Ok(ref mut writer)) => writer,
            None => return Err(Error::Io(io::Error::new(io::ErrorKind::BrokenPipe, "write after close"))),
            Some(Err(_)) => if let Some(Err(e)) = self.writer.take() {
                return Err(e);
            } else { unreachable!(); }
        };

        let filter_result = filter_content(self.filters, content, self.log, self.global, &mut self.filter_storage);

        let write_result = match filter_result {
            Action::Next(Some(ref s)) => {
                let buf = s.as_bytes();
                match writer.write_all(buf) {
                    Ok(()) => Some(Ok(buf.len())),
                    Err(e) => Some(Err(e))
                }
            },
            _ => None
        };

        match write_result {
            Some(Ok(l)) => Ok(l),
            Some(Err(e)) => Err(Error::Io(e)),
            None => match filter_result {
                Action::Abort(e) => Err(Error::Filter(e)),
                Action::Next(None) => Ok(0),
                _ => unreachable!()
            }
        }
    }

    ///Finish writing the response and collect eventual errors.
    ///
    ///This is optional and will happen silently when the writer drops out of
    ///scope.
    pub fn end(mut self) -> Result<(), Error> {
        self.finish()
    }

    fn finish(&mut self) -> Result<(), Error> {
        let mut writer = try!(self.writer.take().expect("can only finish once"));
        let write_queue = try!(filter_end(self.filters, self.log, self.global, &mut self.filter_storage));

        for action in write_queue {
            try!{
                match action {
                    Action::Next(Some(content)) => writer.write_all(content.as_bytes()),
                    Action::Abort(e) => return Err(Error::Filter(e)),
                    _ => Ok(())
                }
            }
        }

        writer.end().map_err(|e| Error::Io(e))
    }

    fn borrow_writer(&mut self) -> Result<&mut hyper::server::response::Response<'a, hyper::net::Streaming>, Error> {
        match self.writer {
            Some(Ok(ref mut writer)) => Ok(writer),
            None => Err(Error::Io(io::Error::new(io::ErrorKind::BrokenPipe, "write after close"))),
            Some(Err(_)) => if let Some(Err(e)) = self.writer.take() {
                Err(e)
            } else { unreachable!(); }
        }
    }
}

impl<'a, 'b> Write for Chunked<'a, 'b> {
    fn write(&mut self, content: &[u8]) -> io::Result<usize> {
        response_to_io_result(self.try_send(content))
    }

    fn write_all(&mut self, content: &[u8]) -> io::Result<()> {
        self.write(content).map(|_| ())
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut writer = try!(response_to_io_result(self.borrow_writer()));
        writer.flush()
    }
}

#[allow(unused_must_use)]
impl<'a, 'b> Drop for Chunked<'a, 'b> {
    ///Finishes writing and closes the connection.
    fn drop(&mut self) {
        if self.writer.is_some() {
            self.finish();
        }
    }
}

///A streaming fixed-size response.
///
///Everything is written directly to the network stream, without being
///filtered, which makes `Raw` especially suitable for transferring files.
///
///__Unsafety__: The content length is set beforehand, which makes it possible
///to send responses that are too short.
pub struct Raw<'a> {
    writer: Option<Result<hyper::server::response::Response<'a, hyper::net::Streaming>, io::Error>>
}

impl<'a> Raw<'a> {
    ///Send a piece of data to the client, ignoring any eventual errors. Use
    ///`try_send` to get error information.
    ///
    ///```
    ///use rustful::{Context, Response};
    ///
    ///fn my_handler(context: Context, response: Response) {
    ///    let count = context.variables.get("count")
    ///        .and_then(|n| n.parse().ok())
    ///        .unwrap_or(0u8);
    ///    let mut raw = unsafe { response.into_raw(count as u64) };
    ///
    ///    for i in 0..count {
    ///        raw.send([i].as_ref());
    ///    }
    ///}
    ///```
    #[allow(unused_must_use)]
    pub fn send<'d, Content: Into<Data<'d>>>(&mut self, content: Content) {
        self.try_send(content);
    }

    ///Send a piece of data to the client. This is the same as `send`, but
    ///errors are not ignored.
    ///
    ///```
    ///use rustful::{Context, Response};
    ///
    ///fn my_handler(context: Context, response: Response) {
    ///    let count = context.variables.get("count")
    ///        .and_then(|n| n.parse().ok())
    ///        .unwrap_or(0u8);
    ///    let mut raw = unsafe { response.into_raw(count as u64) };
    ///
    ///    for i in 0..count {
    ///        if let Err(e) = raw.try_send([i].as_ref()) {
    ///            context.log.note(&format!("failed to write: {}", e));
    ///            break;
    ///        }
    ///    }
    ///}
    ///```
    pub fn try_send<'d, Content: Into<Data<'d>>>(&mut self, content: Content) -> io::Result<()> {
        self.write_all(content.into().as_bytes())
    }

    ///Finish writing the response and collect eventual errors.
    ///
    ///This is optional and will happen silently when the writer drops out of
    ///scope.
    pub fn end(mut self) -> io::Result<()> {
        let writer = match self.writer.take() {
            Some(Ok(writer)) => writer,
            None => return Ok(()), //It has already ended
            Some(Err(e)) => return Err(e)
        };
        writer.end()
    }

    fn borrow_writer(&mut self) -> io::Result<&mut hyper::server::response::Response<'a, hyper::net::Streaming>> {
        match self.writer {
            Some(Ok(ref mut writer)) => Ok(writer),
            None => Err(io::Error::new(io::ErrorKind::BrokenPipe, "write after close")),
            Some(Err(_)) => if let Some(Err(e)) = self.writer.take() {
                Err(e)
            } else { unreachable!(); }
        }
    }
}

impl<'a> Write for Raw<'a> {
    fn write(&mut self, content: &[u8]) -> io::Result<usize> {
        let mut writer = try!(self.borrow_writer());
        writer.write(content)
    }

    fn write_all(&mut self, content: &[u8]) -> io::Result<()> {
        let mut writer = try!(self.borrow_writer());
        writer.write_all(content)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut writer = try!(self.borrow_writer());
        writer.flush()
    }
}

fn response_to_io_result<T>(res:  Result<T, Error>) -> io::Result<T> {
    match res {
        Ok(v) => Ok(v),
        Err(Error::Io(e)) => Err(e),
        Err(e) => Err(io::Error::new(io::ErrorKind::Other, e))
    }
}

fn filter_headers<'a>(
    filters: &'a [Box<ResponseFilter>],
    status: StatusCode,
    headers: &mut Headers,
    log: &Log,
    global: &Global,
    filter_storage: &mut AnyMap
) -> Result<(StatusCode, Vec<Action<'a>>), Error> {
    let mut write_queue = Vec::new();
    let mut header_result = (status, Action::Next(None));

    for filter in filters {
        header_result = match header_result {
            (_, Action::SilentAbort) => break,
            (_, Action::Abort(_)) => break,
            (status, r) => {
                write_queue.push(r);

                let filter_res = {
                    let filter_context = FilterContext {
                        storage: filter_storage,
                        log: log,
                        global: global,
                    };
                    filter.begin(filter_context, status, headers)
                };

                match filter_res {
                    (status, Action::Abort(e)) => (status, Action::Abort(e)),
                    (status, result) => {
                        let mut error = None;
                        
                        write_queue = write_queue.into_iter().filter_map(|action| match action {
                            Action::Next(content) => {
                                let filter_context = FilterContext {
                                    storage: filter_storage,
                                    log: log,
                                    global: global,
                                };
                                Some(filter.write(filter_context, content))
                            },
                            Action::SilentAbort => None,
                            Action::Abort(e) => {
                                error = Some(e);
                                None
                            }
                        }).collect();

                        match error {
                            Some(e) => (status, Action::Abort(e)),
                            None => (status, result)
                        }
                    }
                }
            }
        }
    }

    match header_result {
        (_, Action::Abort(e)) => Err(Error::Filter(e)),
        (status, action) => {
            write_queue.push(action);
            Ok((status, write_queue))
        }
    }
}

fn filter_content<'a, 'd: 'a, Content: Into<Data<'d>>>(filters: &'a [Box<ResponseFilter>], content: Content, log: &Log, global: &Global, filter_storage: &mut AnyMap) -> Action<'a> {
    let mut filter_result = Action::next(Some(content));

    for filter in filters {
        filter_result = match filter_result {
            Action::Next(content) => {
                let filter_context = FilterContext {
                    storage: filter_storage,
                    log: log,
                    global: global,
                };
                filter.write(filter_context, content)
            },
            _ => break
        }
    }

    filter_result
}

fn filter_end<'a>(filters: &'a [Box<ResponseFilter>], log: &Log, global: &Global, filter_storage: &mut AnyMap) -> Result<Vec<Action<'a>>, Error> {
    let otuputs: Vec<_> = filters.into_iter()
        .rev()
        .map(|filter| {
            let filter_context = FilterContext {
                storage: filter_storage,
                log: log,
                global: global,
            };

            filter.end(filter_context)
        })
        .take_while(|a| if let &Action::Next(_) = a { true } else { false })
        .map(|a| Some(a))
        .collect();

    let mut write_queue = vec![];

    for (filter, action) in filters.into_iter().zip(otuputs.into_iter().chain(::std::iter::repeat(None))) {
        let mut error = None;

        write_queue = write_queue.into_iter().filter_map(|action| match action {
            Action::Next(content) => {
                let filter_context = FilterContext {
                    storage: filter_storage,
                    log: log,
                    global: global,
                };
                Some(filter.write(filter_context, content))
            },
            Action::SilentAbort => None,
            Action::Abort(e) => {
                error = Some(e);
                None
            }
        }).collect();

        if let Some(e) = error {
            return Err(Error::Filter(e))
        }

        if let Some(action) = action {
            write_queue.push(action);
        }
    }

    Ok(write_queue)
}