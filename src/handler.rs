//!Request handlers.

#![stable]

use context::Context;
use response::Response;

///A trait for request handlers.
pub trait Handler {
    fn handle_request(&self, context: Context, response: Response);
}

impl<F: Fn(Context, Response)> Handler for F {
    fn handle_request(&self, context: Context, response: Response) {
        self(context, response);
    }
}