//!`Request` is a container for all the request data, including get, set and path variables.

use http::headers::request::HeaderCollection;
use http::method::Method;
use std::hashmap::HashMap;


pub struct Request {
	///Headers from the HTTP request
	headers: ~HeaderCollection,

	///The HTTP method
	method: Method,

	///The requested path
	path: ~str,

	///Route variables
	variables: ~HashMap<~str, ~str>,

	///POST variables
	post: ~HashMap<~str, ~str>,

	///Query variables from the path
	query: ~HashMap<~str, ~str>,

	///The fragment part of the URL (after #)
	fragment: ~str,

	///The raw body part of the request
	body: ~str
}