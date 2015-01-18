use cache::Cache;
use context::Context;
use response::Response;

///A trait for request handlers.
pub trait Handler {
    type Cache;

    fn handle_request(&self, context: Context<Self::Cache>, response: Response);
}

#[old_impl_check]
impl<C: Cache, F: Fn(Context<C>, Response)> Handler for F {
    type Cache = C;

    fn handle_request(&self, context: Context<C>, response: Response) {
        (*self)(context, response);
    }
}