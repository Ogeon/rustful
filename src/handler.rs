//!Request handlers.

use context::Context;
use response::Response;

///A trait for request handlers.
pub trait Handler: Send + Sync + 'static {
    ///Handle a request from the client. Panicking within this method is
    ///discouraged, to allow the server to run smoothly.
    fn handle_request(&self, context: Context, response: Response);
}

impl<F: Fn(Context, Response) + Send + Sync + 'static> Handler for F {
    fn handle_request(&self, context: Context, response: Response) {
        self(context, response);
    }
}