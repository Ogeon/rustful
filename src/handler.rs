//!Request handlers.
use std::borrow::Cow;
use std::sync::mpsc::{Receiver, channel};
use std::sync::Arc;
use std::io::{self, Read, Write};

use context::{Context, RawContext};
use response::{Response, ResponseHead, RawResponse};
use interface::ResponseMessage;

pub use hyper::{Next, Control};

///Combined network writer and HTTP encoder.
#[cfg(not(feature = "ssl"))]
pub type RawEncoder<'a, 'b> = &'a mut ::hyper::Encoder<'b, ::hyper::net::HttpStream>;

///Combined network reader and HTTP decoder.
#[cfg(not(feature = "ssl"))]
pub type RawDecoder<'a, 'b> = &'a mut ::hyper::Decoder<'b, ::hyper::net::HttpStream>;

///Combined network writer and HTTP encoder.
#[cfg(feature = "ssl")]
pub enum RawEncoder<'a, 'b: 'a> {
    ///A writer for HTTP streams.
    Http(&'a mut ::hyper::Encoder<'b, ::hyper::net::HttpStream>),
    ///A writer for HTTPS streams.
    Https(&'a mut ::hyper::Encoder<'b, ::hyper::net::OpensslStream<::hyper::net::HttpStream>>),
}

#[cfg(feature = "ssl")]
impl<'a, 'b> Write for RawEncoder<'a, 'b> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match *self {
            RawEncoder::Http(ref mut encoder) => encoder.write(buf),
            RawEncoder::Https(ref mut encoder) => encoder.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match *self {
            RawEncoder::Http(ref mut encoder) => encoder.flush(),
            RawEncoder::Https(ref mut encoder) => encoder.flush(),
        }
    }
}

#[cfg(feature = "ssl")]
impl<'a, 'b> From<&'a mut ::hyper::Encoder<'b, ::hyper::net::HttpStream>> for RawEncoder<'a, 'b> {
    fn from(encoder: &'a mut ::hyper::Encoder<'b, ::hyper::net::HttpStream>) -> RawEncoder<'a, 'b> {
        RawEncoder::Http(encoder)
    }
}

#[cfg(feature = "ssl")]
impl<'a, 'b> From<&'a mut ::hyper::Encoder<'b, ::hyper::net::OpensslStream<::hyper::net::HttpStream>>> for RawEncoder<'a, 'b> {
    fn from(encoder: &'a mut ::hyper::Encoder<'b, ::hyper::net::OpensslStream<::hyper::net::HttpStream>>) -> RawEncoder<'a, 'b> {
        RawEncoder::Https(encoder)
    }
}

///Combined network reader and HTTP decoder.
#[cfg(feature = "ssl")]
pub enum RawDecoder<'a, 'b: 'a> {
    ///A reader for HTTP streams.
    Http(&'a mut ::hyper::Decoder<'b, ::hyper::net::HttpStream>),
    ///A reader for HTTPS streams.
    Https(&'a mut ::hyper::Decoder<'b, ::hyper::net::OpensslStream<::hyper::net::HttpStream>>),
}

#[cfg(feature = "ssl")]
impl<'a, 'b> Read for RawDecoder<'a, 'b> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match *self {
            RawDecoder::Http(ref mut decoder) => decoder.read(buf),
            RawDecoder::Https(ref mut decoder) => decoder.read(buf),
        }
    }
}

#[cfg(feature = "ssl")]
impl<'a, 'b> From<&'a mut ::hyper::Decoder<'b, ::hyper::net::HttpStream>> for RawDecoder<'a, 'b> {
    fn from(decoder: &'a mut ::hyper::Decoder<'b, ::hyper::net::HttpStream>) -> RawDecoder<'a, 'b> {
        RawDecoder::Http(decoder)
    }
}

#[cfg(feature = "ssl")]
impl<'a, 'b> From<&'a mut ::hyper::Decoder<'b, ::hyper::net::OpensslStream<::hyper::net::HttpStream>>> for RawDecoder<'a, 'b> {
    fn from(decoder: &'a mut ::hyper::Decoder<'b, ::hyper::net::OpensslStream<::hyper::net::HttpStream>>) -> RawDecoder<'a, 'b> {
        RawDecoder::Https(decoder)
    }
}

///A safer HTTP decoder.
pub struct Decoder<'a, 'b: 'a> {
    decoder: RawDecoder<'a, 'b>,
    next: Next,
}

impl<'a, 'b> Decoder<'a, 'b> {
    ///Signal to the event loop that nothing more should be read. There is no
    ///reason to use this under normal circumstances.
    pub fn abort(&mut self) {
        self.next = Next::wait();
    }
}

impl<'a, 'b> Read for Decoder<'a, 'b> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let res = self.decoder.read(buffer);
        self.next = match res {
            Ok(0) => Next::wait(),
            Ok(_) => Next::read(),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Next::read(),
            Err(_) => Next::wait(),
        };

        res
    }
}

///A safer HTTP encoder.
pub struct Encoder<'a, 'b: 'a> {
    encoder: RawEncoder<'a, 'b>,
    next: Next,
}

impl<'a, 'b> Encoder<'a, 'b> {
    ///Signal to the event loop that nothing more should be written. This will
    ///end the response and possibly break the connection, which may lead to
    ///unwanted results. There is no reason to use this under normal
    ///circumstances.
    pub fn abort(&mut self) {
        self.next = Next::end();
    }
}

impl<'a, 'b> Write for Encoder<'a, 'b> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let res = self.encoder.write(buffer);
        self.next = match res {
            Ok(0) => Next::end(),
            //Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Next::write(),
            Err(_) => Next::end(),
            Ok(_) => Next::write(),
        };
        res
    }

    fn flush(&mut self) -> io::Result<()> {
        let res = self.encoder.flush();
        self.next = match res {
            //Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Next::write(),
            Err(_) => Next::end(),
            Ok(_) => Next::write(),
        };
        res
    }
}

///A trait for simple request handlers.
///
///The `Handler` trait makes asynchronous request handling a bit easier, using
///a synchronous-like API. It's still not fully synchronous, so be careful
///with calls to functions that may block.
pub trait Handler: Send + Sync {
    ///Handle a request from the client. Panicking within this method is
    ///discouraged, to allow the server to run smoothly.
    fn handle_request(&self, context: Context, response: Response);

    ///Get a description for the handler.
    fn description(&self) -> Option<Cow<'static, str>> {
        None
    }
}

impl<F: Fn(Context, Response) + Send + Sync> Handler for F {
    fn handle_request(&self, context: Context, response: Response) {
        self(context, response);
    }
}

impl<T: Handler> Handler for Arc<T> {
    fn handle_request(&self, context: Context, response: Response) {
        (**self).handle_request(context, response);
    }
}

impl Handler for Box<Handler> {
    fn handle_request(&self, context: Context, response: Response) {
        (**self).handle_request(context, response);
    }
}

///Access functions for handler meta data.
pub trait Meta {
    ///Get a description for the handler.
    fn description(&self) -> Option<Cow<'static, str>> {
        None
    }
}

impl<H: Handler> Meta for H {
    fn description(&self) -> Option<Cow<'static, str>> {
        self.description()
    }
}

///A fully asynchronous handler.
///
///A raw handler will read and write directly from and to the HTTP decoder and
///encoder, which requires extra care, but will also allow more advanced
///handlers.
pub trait RawHandler: Send {
    ///Return the first state after the request was received. Any necessary
    ///context should be provided through an accompanying `Factory`
    ///implementation.
    fn on_request(&mut self) -> Next;

    ///Read from the request body.
    fn on_request_readable(&mut self, decoder: RawDecoder) -> Next;

    ///Set the response head, including status code and headers.
    fn on_response(&mut self) -> (ResponseHead, Next);

    ///Write to the response body. It's up to the handler, itself, to filter
    ///the response data.
    fn on_response_writable(&mut self, encoder: RawEncoder) -> Next;
}

///A factory for initializing raw handlers.
pub trait Factory: Meta + Send + Sync {
    ///The resulting handler type.
    type Handler: RawHandler;

    ///Initialize a `RawHandler`, using the request context and initial
    ///response data.
    fn create(&self, context: RawContext, response: RawResponse) -> Self::Handler;
}

impl<H: Handler> Factory for H {
    type Handler = HandlerWrapper;

    fn create(&self, context: RawContext, response: RawResponse) -> HandlerWrapper {
        let mut on_body = None;
        let (send, recv) = channel();

        {
            let response = ::interface::response::make_response(
                response,
                send,
                context.control
            );

            let body = ::interface::body::new(&mut on_body, &context.headers);

            let context = Context {
                method: context.method,
                http_version: context.http_version,
                headers: context.headers,
                uri_path: context.uri_path,
                hyperlinks: context.hyperlinks,
                variables: context.variables,
                query: context.query,
                fragment: context.fragment,
                global: context.global,
                body: body,
            };

            self.handle_request(context, response);
        }

        HandlerWrapper::new(on_body, recv)
    }
}

pub struct HandlerWrapper {
    on_body: Option<Box<FnMut(&mut Decoder) + Send>>,
    response_recv: Receiver<ResponseMessage>,
    write_method: Option<WriteMethod>,
}

impl HandlerWrapper {
    fn new(on_body: Option<Box<FnMut(&mut Decoder) + Send>>, recv: Receiver<ResponseMessage>) -> HandlerWrapper {
        HandlerWrapper {
            on_body: on_body,
            response_recv: recv,
            write_method: None,
        }
    }
}

impl RawHandler for HandlerWrapper {
    fn on_request(&mut self) -> Next {
        if self.on_body.is_some() {
            Next::read()
        } else {
            Next::wait()
        }
    }

    fn on_request_readable(&mut self, decoder: RawDecoder) -> Next {
        if let Some(on_body) = self.on_body.as_mut() {
            let mut decoder = Decoder {
                decoder: decoder,
                next: Next::wait(),
            };
            on_body(&mut decoder);
            decoder.next
        } else {
            Next::wait()
        }
    }

    fn on_response(&mut self) -> (ResponseHead, Next) {
        if let Ok(ResponseMessage::Head(head)) = self.response_recv.try_recv() {
            let body_length = if let Some(&::header::ContentLength(length)) = head.headers.get() {
                length as usize
            } else {
                0
            };

            let next = match self.response_recv.try_recv() {
                Ok(ResponseMessage::Buffer(buffer)) => {
                    self.write_method = Some(WriteMethod::Buffer(buffer, 0, body_length));
                    Next::write()
                },
                Ok(ResponseMessage::Callback(on_write)) => {
                    self.write_method = Some(WriteMethod::Callback(on_write));
                    Next::write()
                },
                _ => Next::end()
            };

            (head, next)
        } else {
            panic!("the response didn't send a ResponseHead")
        }
    }

    fn on_response_writable(&mut self, mut encoder: RawEncoder) -> Next {
        match self.write_method {
            Some(WriteMethod::Buffer(ref buffer, ref mut position, max_length)) => {
                while *position < max_length {
                    match encoder.write(&buffer[*position..]) {
                        Ok(0) => {
                            break;
                        },
                        Ok(length) => {
                            *position += length;
                        },
                        Err(e) => if e.kind() == io::ErrorKind::WouldBlock {
                            return Next::write()
                        } else {
                            break
                        },
                    }
                }

                Next::end()
            },
            Some(WriteMethod::Callback(ref mut on_write)) => {
                let mut encoder = Encoder {
                    encoder: encoder,
                    next: Next::end(),
                };
                on_write(&mut encoder);
                encoder.next
            },
            None => Next::end()
        }
    }
}

enum WriteMethod {
    Buffer(Vec<u8>, usize, usize),
    Callback(Box<FnMut(&mut Encoder) + Send>),
}
