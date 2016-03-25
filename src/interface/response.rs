use std::io::{self, Read, Write};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::cmp::min;

use hyper::{Control, Next, Encoder};
use hyper::net::HttpStream;

use anymap::Map;
use anymap::any::Any;

use StatusCode;

use header::{
    Headers,
    ContentType,
};
use filter::{FilterContext, ResponseFilter};
use filter::ResponseAction as Action;
use mime::{Mime, TopLevel, SubLevel};
use server::Global;
use utils::BytesExt;
use response::{Data, Error, FileError};
use interface::{ResponseMessage, ResponseHead};

pub fn make_response(
    raw: RawResponse,
    sender: Sender<ResponseMessage>,
    control: Control,
) -> Response
{
    Response {
        status: raw.status,
        headers: raw.headers,
        sender: sender,
        control: control,
        filters: raw.filters.filters,
        global: raw.filters.global,
        filter_storage: raw.filters.storage,
        sent: false,
    }
}

pub fn make_response_filters(
    filters: Arc<Vec<Box<ResponseFilter>>>,
    global: Arc<Global>,
) -> ResponseFilters {
    ResponseFilters {
        filters: filters,
        global: global,
        storage: Map::new(),
    }
}


///An interface for sending data to the client.
///
///This is where the status code and response headers are set, as well as the
///response body. The body can be directly written through the `Response` if
///its size is known.
pub struct Response {
    ///The response status code. `Ok (200)` is the default.
    pub status: StatusCode,
    ///The response headers.
    pub headers: Headers,
    ///The storage for filter data. This storage is unique for each response
    ///and can be used to communicate with the response filters.
    pub filter_storage: Map<Any + Send + 'static>,

    sender: Sender<ResponseMessage>,
    control: Control,
    filters: Arc<Vec<Box<ResponseFilter>>>,
    global: Arc<Global>,
    sent: bool,
}

impl Response {
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
    ///# #[macro_use] extern crate rustful;
    ///#[macro_use] extern crate log;
    ///use rustful::{Context, Response};
    ///use rustful::response::Error;
    ///
    ///fn my_handler(context: Context, response: Response) {
    ///    if let Err(Error::Filter(e)) = response.try_send("hello") {
    ///        error!("a filter failed: {}", e);
    ///    }
    ///}
    ///
    ///# fn main() {}
    ///```
    pub fn try_send<'d, Content: Into<Data<'d>>>(mut self, content: Content) -> Result<(), Error> {
        self.send_sized(content)
    }

    fn send_sized<'d, Content: Into<Data<'d>>>(&mut self, content: Content) -> Result<(), Error> {
        if self.sent {
            panic!("response used after drop");
        }

        self.sent = true;

        let buffer = if self.filters.is_empty() {
            content.into().into()
        } else {
            let mut buffer = vec![];

            let (status, write_queue) = try!(filter_headers(
                &self.filters,
                ::std::mem::replace(&mut self.status, StatusCode::Ok),
                &mut self.headers,
                &self.global,
                &mut self.filter_storage
            ));
            self.status = status;
            for action in write_queue {
                match action {
                    Action::Next(Some(content)) => buffer.push_bytes(content.as_bytes()),
                    Action::Next(None) => {},
                    Action::Abort(e) => return Err(Error::Filter(e)),
                    Action::SilentAbort => break
                }
            }

            let filter_result = filter_content(&self.filters, content, &self.global, &mut self.filter_storage);
            match filter_result {
                Action::Next(Some(content)) => buffer.push_bytes(content.as_bytes()),
                Action::Abort(e) => return Err(Error::Filter(e)),
                _ => {}
            }

            let write_queue = try!(filter_end(&self.filters, &self.global, &mut self.filter_storage));
            for action in write_queue {
                match action {
                    Action::Next(Some(content)) => buffer.push_bytes(content.as_bytes()),
                    Action::Next(None) => {},
                    Action::Abort(e) => return Err(Error::Filter(e)),
                    Action::SilentAbort => break
                }
            }
            
            buffer
        };

        self.headers.set(::header::ContentLength(buffer.len() as u64));
        
        let _ = self.sender.send(ResponseMessage::Head(ResponseHead {
            status: ::std::mem::replace(&mut self.status, StatusCode::Ok),
            headers: ::std::mem::replace(&mut self.headers, Headers::new()),
        }));
        let _ = self.sender.send(ResponseMessage::Buffer(buffer));
        self.control.ready(Next::write());
        Ok(())
    }

    ///Send a static file to the client.
    ///
    ///A MIME type is automatically applied to the response, based on the file
    ///extension, and `application/octet-stream` is used as a fallback if the
    ///extension is unknown. Use `send_file_with_mime` to override the MIME
    ///guessing. See also [`ext_to_mime`](../file/fn.ext_to_mime.html) for more
    ///information.
    ///
    ///An error is returned upon failure and the response may be recovered
    ///from there if the file could not be opened.
    ///
    ///```
    ///# #[macro_use] extern crate rustful;
    ///#[macro_use] extern crate log;
    ///use std::path::Path;
    ///use rustful::{Context, Response};
    ///use rustful::StatusCode;
    ///use rustful::file::check_path;
    ///use rustful::response::FileError;
    ///
    ///fn my_handler(mut context: Context, mut response: Response) {
    ///    if let Some(file) = context.variables.get("file") {
    ///        let file_path = Path::new(file.as_ref());
    ///
    ///        //Check if the path is valid
    ///        if check_path(file_path).is_ok() {
    ///            //Make a full path from the filename
    ///            let path = Path::new("path/to/files").join(file_path);
    ///
    ///            //Send the file
    ///            let res = response.send_file(&path)
    ///                .or_else(|e| e.send_not_found("the file was not found"));
    ///
    ///            //Check if a more fatal file error than "not found" occurred
    ///            if let Err(FileError { error, mut response }) = res {
    ///                //Something went horribly wrong
    ///                error!("failed to open '{}': {}", file, error);
    ///                response.status = StatusCode::InternalServerError;
    ///            }
    ///        } else {
    ///            //Accessing parent directories is forbidden
    ///            response.status = StatusCode::Forbidden;
    ///        }
    ///    } else {
    ///        //No filename was specified
    ///        response.status = StatusCode::Forbidden;
    ///    }
    ///}
    ///# fn main() {}
    ///```
    pub fn send_file<P: AsRef<Path>>(self, path: P) -> Result<(), FileError> {
        self.send_file_with_mime(path, ::file::ext_to_mime)
    }


    ///Send a static file with a specified MIME type to the client.
    ///
    ///This can be used instead of `send_file` to control what MIME type the
    ///file will be sent as. This can be useful if, for example, the MIME guesser
    ///happens to be wrong about some file extension.
    ///
    ///An error is returned upon failure and the response may be recovered
    ///from there if the file could not be opened.
    ///
    ///```
    ///# #[macro_use] extern crate rustful;
    ///#[macro_use] extern crate log;
    ///use std::path::Path;
    ///use rustful::{Context, Response};
    ///use rustful::StatusCode;
    ///use rustful::file;
    ///use rustful::response::FileError;
    ///
    ///fn my_handler(mut context: Context, mut response: Response) {
    ///    if let Some(file) = context.variables.get("file") {
    ///        let file_path = Path::new(file.as_ref());
    ///
    ///        //Check if the path is valid
    ///        if file::check_path(file_path).is_ok() {
    ///            //Make a full path from the filename
    ///            let path = Path::new("path/to/files").join(file_path);
    ///
    ///            //Send .rs files as Rust files and do the usual guessing for the rest
    ///            let res = response.send_file_with_mime(&path, |ext| {
    ///                if ext == "rs" {
    ///                    Some(content_type!(Text / "rust"; Charset = Utf8))
    ///                } else {
    ///                    file::ext_to_mime(ext)
    ///                }
    ///            }).or_else(|e| e.send_not_found("the file was not found"));
    ///
    ///            //Check if a more fatal file error than "not found" occurred
    ///            if let Err(FileError { error, mut response }) = res {
    ///                //Something went horribly wrong
    ///                error!("failed to open '{}': {}", file, error);
    ///                response.status = StatusCode::InternalServerError;
    ///            }
    ///        } else {
    ///            //Accessing parent directories is forbidden
    ///            response.status = StatusCode::Forbidden;
    ///        }
    ///    } else {
    ///        //No filename was specified
    ///        response.status = StatusCode::Forbidden;
    ///    }
    ///}
    ///# fn main() {}
    ///```
    pub fn send_file_with_mime<P, F>(mut self, path: P, to_mime: F) -> Result<(), FileError> where
        P: AsRef<Path>,
        F: FnOnce(&str) -> Option<Mime>
    {
        const MAX_BUFFER_SIZE: usize = 2048;

        let path: &Path = path.as_ref();
        let mime = path
            .extension()
            .and_then(|ext| to_mime(&ext.to_string_lossy()))
            .unwrap_or(Mime(TopLevel::Application, SubLevel::Ext("octet-stream".into()), vec![]));

        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(e) => return Err(FileError { error: e, response: self })
        };
        let metadata = match file.metadata() {
            Ok(metadata) => metadata,
            Err(e) => return Err(FileError { error: e, response: self })
        };

        self.headers.set(ContentType(mime));

        let mut buffer = Vec::with_capacity(MAX_BUFFER_SIZE);
        let mut read_pos = 0;
        let mut written = 0;
        let file_size = metadata.len() as usize;

        unsafe { self.raw_send(metadata.len(), move |writer| {
            if read_pos >= buffer.len() {
                buffer.resize(min(MAX_BUFFER_SIZE, file_size - written), 0);
                let len = try!(file.read(&mut buffer[..]));
                buffer.truncate(len);
                read_pos = 0;
            }

            let len = try!(writer.write(&buffer[read_pos..]));
            read_pos += len;
            written += len;
            Ok(len)
        }) };

        Ok(())
    }

    /*///Write the status code and headers to the client and turn the `Response`
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
            self.global,
            self.filter_storage_mut()
        ).and_then(|(status, write_queue)|{
            if self.force_close {
                writer.headers_mut().set(Connection(vec![ConnectionOption::Close]));
            }
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

        if self.force_close {
            writer.headers_mut().set(Connection(vec![ConnectionOption::Close]));
        }
        writer.headers_mut().remove_raw("content-length");
        writer.headers_mut().set(::header::ContentLength(content_length));

        Raw {
            writer: Some(writer.start())
        }
    }*/


    ///Write the status code and headers to the client and register an
    ///`on_write` callback. Any eventual response filters are bypassed to make
    ///sure that the data is not modified.
    ///
    ///__Unsafety__: The content length is set beforehand, which makes it
    ///possible to send responses that are too short.
    pub unsafe fn raw_send<F>(mut self, content_length: u64, on_write: F) where
        F: FnMut(&mut Encoder<HttpStream>) -> io::Result<usize> + Send + 'static
    {
        self.sent = true;

        self.headers.remove_raw("content-length");
        self.headers.set(::header::ContentLength(content_length));
        
        let _ = self.sender.send(ResponseMessage::Head(ResponseHead {
            status: ::std::mem::replace(&mut self.status, StatusCode::Ok),
            headers: ::std::mem::replace(&mut self.headers, Headers::new()),
        }));
        let _ = self.sender.send(ResponseMessage::Callback(Box::new(on_write)));
        self.control.ready(Next::write());
    }
}

#[allow(unused_must_use)]
impl Drop for Response {
    ///Writes status code and headers and closes the connection.
    fn drop(&mut self) {
        if !self.sent {
            self.send_sized(&[][..]);
        }
    }
}


/*///An interface for writing a chunked response body.
///

///This is useful for when the size of the data is unknown, but it comes with
///an overhead for each time `send` or `try_send` is called (simply put).
pub struct Chunked<'a, 'b> {
    writer: Option<Result<hyper::server::response::Response<'a, hyper::net::Streaming>, Error>>,
    filters: &'b [Box<ResponseFilter>],
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
    ///# #[macro_use] extern crate rustful;
    ///#[macro_use] extern crate log;
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
    ///            error!("a filter failed: {}", e);
    ///        }
    ///    }
    ///}
    ///# fn main() {}
    ///```
    pub fn try_send<'d, Content: Into<Data<'d>>>(&mut self, content: Content) -> Result<usize, Error> {
        let mut writer = match self.writer {
            Some(Ok(ref mut writer)) => writer,
            None => return Err(Error::Io(io::Error::new(io::ErrorKind::BrokenPipe, "write after close"))),
            Some(Err(_)) => if let Some(Err(e)) = self.writer.take() {
                return Err(e);
            } else { unreachable!(); }
        };

        let filter_result = filter_content(self.filters, content, self.global, &mut self.filter_storage);

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
        let write_queue = try!(filter_end(self.filters, self.global, &mut self.filter_storage));

        for action in write_queue {
            try!{
                match action {
                    Action::Next(Some(content)) => writer.write_all(content.as_bytes()),
                    Action::Abort(e) => return Err(Error::Filter(e)),
                    _ => Ok(())
                }
            }
        }

        writer.end().map_err(Error::Io)
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
}*/

/*///A streaming fixed-size response.
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
    ///# fn main() {}
    ///```
    #[allow(unused_must_use)]
    pub fn send<'d, Content: Into<Data<'d>>>(&mut self, content: Content) {
        self.try_send(content);
    }

    ///Send a piece of data to the client. This is the same as `send`, but
    ///errors are not ignored.
    ///
    ///```
    ///# #[macro_use] extern crate rustful;
    ///#[macro_use] extern crate log;
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
    ///            error!("failed to write: {}", e);
    ///            break;
    ///        }
    ///    }
    ///}
    ///# fn main() {}
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
}*/

///A stripped down HTTP response, for `RawHandler`.
pub struct RawResponse {
    ///The response status code. `Ok (200)` is the default.
    pub status: StatusCode,

    ///The response headers.
    pub headers: Headers,

    ///The response filter stack.
    pub filters: ResponseFilters,
}

///A standalone `ResponseFilter` stack.
///
///It can be used to filter response data before sending it to the client.
pub struct ResponseFilters {
    ///The storage for filter data. This storage is unique for each response
    ///and can be used to communicate with the response filters.
    pub storage: Map<Any + Send + 'static>,
    global: Arc<Global>,
    filters: Arc<Vec<Box<ResponseFilter>>>,
}

impl ResponseFilters {
    ///Set or modify headers before they are sent to the client and maybe
    ///initiate the body.
    pub fn begin(&mut self, status: StatusCode, headers: &mut Headers) -> Result<(StatusCode, Vec<Data>), Error> {
        let (status, actions) = try!(filter_headers(&self.filters, status, headers, &self.global, &mut self.storage));
        let data: Result<_, _> = actions.into_iter().filter_map(|action| match action {
            Action::SilentAbort => None,
            Action::Abort(e) => Some(Err(Error::Filter(e))),
            Action::Next(data) => data.map(Ok),
        }).collect();
        data.map(|d| (status, d))
    }

    ///Handle content before writing it to the body.
    pub fn body<'a: 'b, 'b, Content: Into<Data<'a>>>(&'b mut self, content: Content) -> Result<Option<Data<'b>>, Error> {
        match filter_content(&self.filters, content, &self.global, &mut self.storage) {
            Action::SilentAbort => Ok(None),
            Action::Abort(e) => Err(Error::Filter(e)),
            Action::Next(data) => Ok(data),
        }
    }

    ///End of body writing. Last chance to add content.
    pub fn end(&mut self) -> Result<Vec<Data>, Error> {
        try!(filter_end(&self.filters, &self.global, &mut self.storage)).into_iter().filter_map(|action| match action {
            Action::SilentAbort => None,
            Action::Abort(e) => Some(Err(Error::Filter(e))),
            Action::Next(data) => data.map(Ok),
        }).collect()
    }
}

fn filter_headers<'a>(
    filters: &'a [Box<ResponseFilter>],
    status: StatusCode,
    headers: &mut Headers,
    global: &Global,
    filter_storage: &mut Map<Any + Send + 'static>
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

fn filter_content<'a: 'b, 'b, Content: Into<Data<'a>>>(filters: &'b [Box<ResponseFilter>], content: Content, global: &Global, filter_storage: &mut Map<Any + Send + 'static>) -> Action<'b> {
    let mut filter_result = Action::next(Some(content));

    for filter in filters {
        filter_result = match filter_result {
            Action::Next(content) => {
                let filter_context = FilterContext {
                    storage: filter_storage,
                    global: global,
                };
                filter.write(filter_context, content)
            },
            _ => break
        }
    }

    filter_result
}

fn filter_end<'a>(filters: &'a [Box<ResponseFilter>], global: &Global, filter_storage: &mut Map<Any + Send + 'static>) -> Result<Vec<Action<'a>>, Error> {
    let otuputs: Vec<_> = filters.into_iter()
        .rev()
        .map(|filter| {
            let filter_context = FilterContext {
                storage: filter_storage,
                global: global,
            };

            filter.end(filter_context)
        })
        .take_while(|a| if let Action::Next(_) = *a { true } else { false })
        .map(Some)
        .collect();

    let mut write_queue = vec![];

    for (filter, action) in filters.into_iter().zip(otuputs.into_iter().chain(::std::iter::repeat(None))) {
        let mut error = None;

        write_queue = write_queue.into_iter().filter_map(|action| match action {
            Action::Next(content) => {
                let filter_context = FilterContext {
                    storage: filter_storage,
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
