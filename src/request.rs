//!`Request` is a container for all the request data, including get, set and path variables.

use http::headers::request::HeaderCollection;
use std::hashmap::HashMap;


pub struct Request {
	///Headers from the HTTP request
	headers: ~HeaderCollection,

	///The requested path
	path: ~str,

	///Route variables
	variables: ~HashMap<~str, ~str>
}