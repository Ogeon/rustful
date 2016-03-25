//!Request handlers.
use std::borrow::Cow;
use std::sync::mpsc::{Receiver, channel};
use std::sync::Arc;
use std::io::{self, Read, Write};
use std::cmp::min;

use context::{Context, RawContext};
use context::body::BodyReader;
use response::{Response, ResponseHead, RawResponse};
use utils::FnBox;
use interface::ResponseMessage;

const MAX_BUFFER_LENGTH: usize = 2048;

pub use hyper::{Next, Control};

pub type Encoder<'a> = ::hyper::Encoder<'a, ::hyper::net::HttpStream>;
pub type Decoder<'a> = ::hyper::Decoder<'a, ::hyper::net::HttpStream>;

///A trait for simple request handlers.
///
///The `Handler` trait makes asynchronous request handling a bit easier, using
///a synchronous-like API. It's still not fully synchronous, so be careful
///with calls to functions that may block.
pub trait Handler: Send + Sync + 'static {
    ///Handle a request from the client. Panicking within this method is
    ///discouraged, to allow the server to run smoothly.
    fn handle_request(&self, context: Context, response: Response);

    ///Get a description for the handler.
    fn description(&self) -> Option<Cow<'static, str>> {
        None
    }
}

impl<F: Fn(Context, Response) + Send + Sync + 'static> Handler for F {
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
pub trait RawHandler: Send + 'static {
    ///Return the first state after the request was received. Any necessary
    ///context should be provided through an accompanying `Factory`
    ///implementation.
    fn on_request(&mut self) -> Next;

    ///Read from the request body.
    fn on_request_readable(&mut self, decoder: &mut Decoder) -> Next;

    ///Set the response head, including status code and headers.
    fn on_response(&mut self) -> (ResponseHead, Next);

    ///Write to the response body. It's up to the handler, itself, to filter
    ///the response data.
    fn on_response_writable(&mut self, encoder: &mut Encoder) -> Next;
}

///A factory for initializing raw handlers.
pub trait Factory: Meta + Send + Sync + 'static {
    ///The resulting handler type.
    type Handler: RawHandler;

    ///Initialize a `RawHandler`, using the request context and initial
    ///response data.
    fn create(&self, context: RawContext, response: RawResponse) -> Self::Handler;
}

impl<H: Handler> Factory for H {
    type Handler = RawWrapper;

    fn create(&self, context: RawContext, response: RawResponse) -> RawWrapper {
        let mut on_body = None;
        let (send, recv) = channel();

        let body_length = if let Some(&::header::ContentLength(length)) = context.request.headers().get() {
            length as usize
        } else {
            0
        };

        {
            let response = ::interface::response::make_response(
                response,
                send,
                context.control
            );

            let context = Context {
                request: context.request,
                uri_path: context.uri_path,
                hyperlinks: context.hyperlinks,
                variables: context.variables,
                query: context.query,
                fragment: context.fragment,
                global: context.global,
                body: BodyReader::new(&mut on_body),
            };

            self.handle_request(context, response);
        }

        RawWrapper::new(on_body, body_length, recv)
    }
}

pub struct RawWrapper {
    on_body: Option<Box<FnBox<io::Result<Vec<u8>>, Output=()> + Send>>,
    body_buffer: Vec<u8>,
    body_length: usize,

    response_recv: Receiver<ResponseMessage>,
    write_method: Option<WriteMethod>,
}

impl RawWrapper {
    fn new(on_body: Option<Box<FnBox<io::Result<Vec<u8>>, Output=()> + Send>>, body_length: usize, recv: Receiver<ResponseMessage>) -> RawWrapper {
        RawWrapper {
            on_body: on_body,
            body_buffer: vec![],
            body_length: body_length,

            response_recv: recv,
            write_method: None,
        }
    }
}

impl RawHandler for RawWrapper {
    fn on_request(&mut self) -> Next {
        if self.on_body.is_some() {
            Next::read()
        } else {
            Next::wait()
        }
    }

    fn on_request_readable(&mut self, decoder: &mut Decoder) -> Next {
        while self.body_buffer.len() < self.body_length {
            let read_len = min(self.body_length - self.body_buffer.len(), MAX_BUFFER_LENGTH);
            let start = self.body_buffer.len();
            let new_len = self.body_buffer.len() + read_len;
            self.body_buffer.resize(new_len, 0);

            match decoder.read(&mut self.body_buffer[start..]) {
                Ok(0) => {
                    self.body_buffer.truncate(start);
                    break;
                },
                Ok(length) => {
                    self.body_buffer.truncate(start + length);
                },
                Err(e) => if e.kind() == io::ErrorKind::WouldBlock {
                    self.body_buffer.truncate(start);
                    return Next::read()
                } else if let Some(on_body) = self.on_body.take() {
                    on_body.call_box(Err(e));
                    return Next::wait()
                } else {
                    return Next::wait()
                }
            }
        }

        if let Some(on_body) = self.on_body.take() {
            self.body_length = 0;
            on_body.call_box(Ok(::std::mem::replace(&mut self.body_buffer, vec![])));
        }

        Next::wait()
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

    fn on_response_writable(&mut self, encoder: &mut Encoder) -> Next {
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
            Some(WriteMethod::Callback(ref mut on_write)) => match on_write(encoder) {
                Ok(0) => Next::end(),
                //Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Next::write(),
                Err(_) => Next::end(),
                Ok(_) => Next::write(),
            },
            None => Next::end()
        }
    }
}

enum WriteMethod {
    Buffer(Vec<u8>, usize, usize),
    Callback(Box<FnMut(&mut Encoder) -> io::Result<usize> + Send>),
}
