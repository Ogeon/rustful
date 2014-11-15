use std::collections::HashMap;

use utils;
use Request;

pub trait QueryBody {
    fn parse_query_body(&self) -> HashMap<String, String>;
}

impl QueryBody for Request {
    ///Parse the request body as a query string.
    ///The body will be decoded as UTF-8 and plain '+' characters will be replaced with spaces.
    #[inline]
    fn parse_query_body(&self) -> HashMap<String, String> {
        utils::parse_parameters(self.body.as_slice())
    }
}