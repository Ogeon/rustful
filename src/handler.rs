//!Request handlers.
use std::borrow::Cow;

use context::Context;
use response::Response;
use std::sync::Arc;

///A trait for request handlers.
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
