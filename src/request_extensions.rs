use std::collections::HashMap;
use std::io::IoResult;

use utils;
use Request;

pub trait QueryBody {
    fn read_query_body(&mut self) -> IoResult<HashMap<String, String>>;
}

impl<'a> QueryBody for Request<'a> {
    ///Rad and parse the request body as a query string.
    ///The body will be decoded as UTF-8 and plain '+' characters will be replaced with spaces.
    #[inline]
    fn read_query_body(&mut self) -> IoResult<HashMap<String, String>> {
        Ok(utils::parse_parameters(try!(self.read_to_end()).as_slice()))
    }
}