//!`Request` is a container for all the request data, including get, set and path variables.

use http::headers::request::HeaderCollection;
use http::method::Method;
use collections::hashmap::HashMap;


pub struct Request {
	///Headers from the HTTP request
	pub headers: HeaderCollection,

	///The HTTP method
	pub method: Method,

	///The requested path
	pub path: ~str,

	///Route variables
	pub variables: HashMap<~str, ~str>,

	///POST variables
	pub post: HashMap<~str, ~str>,

	///Query variables from the path
	pub query: HashMap<~str, ~str>,

	///The fragment part of the URL (after #)
	pub fragment: ~str,

	///The raw body part of the request
	pub body: ~str
}