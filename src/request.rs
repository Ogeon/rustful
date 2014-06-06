//!`Request` is a container for all the request data, including get, set and path variables.

use http::headers::request::HeaderCollection;
use http::method::Method;
use std::collections::hashmap::HashMap;


pub struct Request<'a> {
	///Headers from the HTTP request
	pub headers: HeaderCollection,

	///The HTTP method
	pub method: Method,

	///The requested path
	pub path: &'a str,

	///Route variables
	pub variables: HashMap<String, String>,

	///POST variables
	pub post: HashMap<String, String>,

	///Query variables from the path
	pub query: HashMap<String, String>,

	///The fragment part of the URL (after #), if provided
	pub fragment: Option<&'a str>,

	///The raw body part of the request
	pub body: &'a str
}