//!`Router` takes care of routing incoming requests to handlers.
//!
//!```rust
//!# use rustful::router::Router;
//!# use std::hashmap::HashMap;
//!# fn about_us(_: ~HashMap<~str, &str>) -> ~str {~""}
//!# fn show_user(_: ~HashMap<~str, &str>) -> ~str {~""}
//!# fn show_product(_: ~HashMap<~str, &str>) -> ~str {~""}
//!# fn show_error(_: ~HashMap<~str, &str>) -> ~str {~""}
//!# fn show_welcome(_: ~HashMap<~str, &str>) -> ~str {~""}
//!let routes = [
//!	("/about", about_us),
//!	("/user/:user", show_user),
//!	("/product/:name", show_product),
//!	("/*", show_error),
//!	("/", show_welcome)
//!];
//!
//!let router = Router::from_vec(routes);
//!```
use std::hashmap::HashMap;
use request::Request;


///A handler function for routing.
pub type HandlerFn = fn(&Request) -> ~str;


///Takes care of routing requests to handlers.
///
///Paths can be static (`"path/to/handler"`) or variable (`"users/:group/:user"`)
///and contain wildcards (`"path/*/handler/*"`).
///
///Variables (starting with `:`) will match whatever word the request path contains at
///that point and it will be sent as a value to the handler function.
///
///Wildcards (a single `*`) will consume the segments until the rest of the
///path gives a match.
///
///```
///pattern = "a/*/b"
///"a/c/b" -> match
///"a/c/d/b" -> match
///"a/b" -> no match
///"a/c/b/d" -> no match
///```
///
///```
///pattern = "a/b/*"
///"a/b/c" -> match
///"a/b/c/d" -> match
///"a/b" -> no match
///```
#[deriving(Clone)]
pub struct Router {
	priv handler: Option<HandlerFn>,
	priv static_routes: HashMap<~str, ~Router>,
	priv variable_route: Option<~Router>,
	priv variable_names: ~[~str],
	priv wildcard_route: Option<~Router>
}

impl Router {
	///Creates an empty `Router`.
	pub fn new() -> Router {
		Router {
			handler: None,
			static_routes: HashMap::new(),
			variable_route: None,
			variable_names: ~[],
			wildcard_route: None
		}
	}

	///Generates a `Router` tree from a set of handlers and paths.
	pub fn from_vec(routes: &[(&str, HandlerFn)]) -> Router {
		let mut root = Router::new();

		for &(path, handler) in routes.iter() {
			root.insert_handler(path, handler);
		}

		root
	}

	///Inserts a handler into the `Router` at a given path.
	pub fn insert_handler(&mut self, path: &str, handler: HandlerFn) {
		self.insert_handler_vec(Router::path_to_vec(path.trim()), ~[], handler);
	}

	//Same as `insert_handler`, but internal
	fn insert_handler_vec(&mut self, path: &[&str], variable_names: ~[~str], handler: HandlerFn) {
		let mut var_names = variable_names;

		match path {
			[piece] => {
				let next = self.find_or_insert_router(piece);
				if piece.len() > 0 && piece.char_at(0) == ':' {
					var_names.push(piece.slice(1, piece.len()).to_owned());
				}
				next.variable_names = var_names;
				next.handler = Some(handler);
			},
			[piece, ..rest] => {
				let next = self.find_or_insert_router(piece);
				if piece.len() > 0 && piece.char_at(0) == ':' {
					var_names.push(piece.slice(1, piece.len()).to_owned());
				}
				next.insert_handler_vec(rest, var_names, handler);
			},
			[] => {
				self.variable_names = var_names;
				self.handler = Some(handler);
			}
		}
	}

	//Tries to find a router matching the key or inserts a new one if none exists
	fn find_or_insert_router<'a>(&'a mut self, key: &str) -> &'a mut ~Router {
		if key == "*" {
			if self.wildcard_route.is_none() {
				self.wildcard_route = Some(~Router::new());
			}
			self.wildcard_route.as_mut::<'a>().unwrap()
		} else if key.len() > 0 && key.char_at(0) == ':' {
			if self.variable_route.is_none() {
				self.variable_route = Some(~Router::new());
			}
			self.variable_route.as_mut::<'a>().unwrap()
		} else {
			self.static_routes.find_or_insert_with::<'a>(key.to_owned(), |_| {
				~Router::new()
			})
		}
	}

	///Executes a matching handler function and returns the result.
	pub fn route(&self, request: ~Request) -> Option<~str> {
		let mut request = request;
		self.find(Router::path_to_vec(request.path.to_owned()), &[], request)
	}

	//Tries to find a matching handler and run it
	fn find(&self, path: &[&str], variables: &[&str], request: &mut Request) -> Option<~str> {
		match path {
			[piece] => {
				self.match_static(piece, &[], variables, |next, _, vars| { next.exec(vars, request) })
			},
			[piece, ..rest] => {
				self.match_static(piece, rest, variables, |next, path, vars| { next.find(path, vars, request) })
			},
			[] => {
				self.exec(variables, request)
			}
		}
	}

	//Tries to run a handler with a given set of variable values
	fn exec(&self, variables: &[&str], request: &mut Request) -> Option<~str> {
		match self.handler {
			Some(handler) => {
				for (key, &value) in self.variable_names.iter().zip(variables.iter()) {
					request.variables.insert(key.to_owned(), value.to_owned());
				}
				Some(handler(request))
			},
			None => None
		}
	}

	//Checks for a static route. Runs `action` if found, runs `match_variable` otherwhise
	fn match_static(&self, key: &str, rest: &[&str], variables: &[&str], action: |&~Router, &[&str], &[&str]| -> Option<~str>) -> Option<~str> {
		match self.static_routes.find(&key.to_owned()) {
			Some(next) => {
				match action(next, rest, variables) {
					None => self.match_variable(key, rest, variables, action),
					result => result
				}
			},
			None => self.match_variable(key, rest, variables, action)
		}
	}

	//Checks for a variable route. Runs `action` if found, runs `match_wildcard` otherwhise
	fn match_variable(&self, key: &str, rest: &[&str], variables: &[&str], action: |&~Router, &[&str], &[&str]| -> Option<~str>) -> Option<~str> {
		match self.variable_route {
			Some(ref next) => {
				let mut new_variables = variables.to_owned();
				new_variables.push(key.clone());

				match action(next, rest, new_variables) {
					None => self.match_wildcard(rest, variables, action),
					result => result
				}
			},
			None => self.match_wildcard(rest, variables, action)
		}
	}

	//Checks for a wildcard route. Runs `action` if found, returns `None` otherwhise
	fn match_wildcard(&self, rest: &[&str], variables: &[&str], action: |&~Router, &[&str], &[&str]| -> Option<~str>) -> Option<~str> {
		match self.wildcard_route {
			Some(ref next) => {
				let mut path = rest;
				while path.len() > 0 {
					match action(next, path, variables) {
						None => path = path.slice(1, path.len()),
						result => return result
					}
				}

				action(next, path, variables)
			},
			None => None
		}
	}

	//Converts a path to a suitable array of path segments
	fn path_to_vec<'a>(path: &'a str) -> ~[&'a str] {
		if path.len() == 0 {
			~[]
		} else if path.len() == 1 {
			if path == "/" {
				~[]
			} else {
				~[path]
			}
		} else {
			let start = if path.char_at(0) == '/' { 1 } else { 0 };
			let end = if path.char_at(path.len() - 1) == '/' { 1 } else { 0 };
			path.slice(start, path.len() - end).split('/').collect::<~[&str]>()
		}
	}
}



#[cfg(test)]
mod test {
	use extra::test::BenchHarness;
	use std::hashmap::HashMap;
	use super::Router;
	use http::headers::request::HeaderCollection;
	use request::Request;

	fn request(path: ~str) -> ~Request {
		~Request {
			headers: ~HeaderCollection::new(),
			path: path,
			variables: HashMap::new()
		}
	}

	fn test_1(_: &Request) -> ~str {
		~"test 1"
	}

	fn test_2(_: &Request) -> ~str {
		~"test 2"
	}

	fn test_3(_: &Request) -> ~str {
		~"test 3"
	}

	fn test_var(request: &Request) -> ~str {
		let keys = ~[~"a", ~"b", ~"c"];
		keys.iter().filter_map(|key| {
			match request.variables.find(key) {
				Some(value) => Some(value.to_owned()),
				None => None
			}
		}).collect::<~[~str]>().connect(", ")
	}

	#[test]
	fn one_static_route() {
		let routes = [("path/to/test1", test_1)];

		let router = Router::from_vec(routes);

		assert_eq!(router.route(request(~"path/to/test1")), Some(~"test 1"));
		assert_eq!(router.route(request(~"path/to")), None);
		assert_eq!(router.route(request(~"path/to/test1/nothing")), None);
	}

	#[test]
	fn several_static_routes() {
		let routes = [
			("", test_1),
			("path/to/test/no2", test_2),
			("path/to/test1/no/test3", test_3)
		];

		let router = Router::from_vec(routes);

		assert_eq!(router.route(request(~"")), Some(~"test 1"));
		assert_eq!(router.route(request(~"path/to/test/no2")), Some(~"test 2"));
		assert_eq!(router.route(request(~"path/to/test1/no/test3")), Some(~"test 3"));
		assert_eq!(router.route(request(~"path/to/test1/no")), None);
	}

	#[test]
	fn one_variable_route() {
		let routes = [("path/:a/test1", test_var)];

		let router = Router::from_vec(routes);

		assert_eq!(router.route(request(~"path/to/test1")), Some(~"to"));
		assert_eq!(router.route(request(~"path/to")), None);
		assert_eq!(router.route(request(~"path/to/test1/nothing")), None);
	}

	#[test]
	fn several_variable_routes() {
		let routes = [
			("path/to/test1", test_var),
			("path/:a/test/no2", test_var),
			("path/to/:b/:c/:a", test_var)
		];

		let router = Router::from_vec(routes);

		assert_eq!(router.route(request(~"path/to/test1")), Some(~""));
		assert_eq!(router.route(request(~"path/to/test/no2")), Some(~"to"));
		assert_eq!(router.route(request(~"path/to/test1/no/test3")), Some(~"test3, test1, no"));
		assert_eq!(router.route(request(~"path/to/test1/no")), None);
	}

	#[test]
	fn one_wildcard_end_route() {
		let routes = [("path/to/*", test_1)];

		let router = Router::from_vec(routes);

		assert_eq!(router.route(request(~"path/to/test1")), Some(~"test 1"));
		assert_eq!(router.route(request(~"path/to/same/test1")), Some(~"test 1"));
		assert_eq!(router.route(request(~"path/to/the/same/test1")), Some(~"test 1"));
		assert_eq!(router.route(request(~"path/to")), None);
		assert_eq!(router.route(request(~"path")), None);
	}


	#[test]
	fn one_wildcard_middle_route() {
		let routes = [("path/*/test1", test_1)];

		let router = Router::from_vec(routes);

		assert_eq!(router.route(request(~"path/to/test1")), Some(~"test 1"));
		assert_eq!(router.route(request(~"path/to/same/test1")), Some(~"test 1"));
		assert_eq!(router.route(request(~"path/to/the/same/test1")), Some(~"test 1"));
		assert_eq!(router.route(request(~"path/to")), None);
		assert_eq!(router.route(request(~"path")), None);
	}

	#[test]
	fn one_universal_wildcard_route() {
		let routes = [("*", test_1)];

		let router = Router::from_vec(routes);

		assert_eq!(router.route(request(~"path/to/test1")), Some(~"test 1"));
		assert_eq!(router.route(request(~"path/to/same/test1")), Some(~"test 1"));
		assert_eq!(router.route(request(~"path/to/the/same/test1")), Some(~"test 1"));
		assert_eq!(router.route(request(~"path/to")), Some(~"test 1"));
		assert_eq!(router.route(request(~"path")), Some(~"test 1"));
		assert_eq!(router.route(request(~"")), None);
	}

	#[test]
	fn several_wildcards_routes() {
		let routes = [
			("path/to/*", test_1),
			("path/*/test/no2", test_2),
			("path/to/*/*/*", test_3)
		];

		let router = Router::from_vec(routes);

		assert_eq!(router.route(request(~"path/to/test1")), Some(~"test 1"));
		assert_eq!(router.route(request(~"path/for/test/no2")), Some(~"test 2"));
		assert_eq!(router.route(request(~"path/to/test1/no/test3")), Some(~"test 3"));
		assert_eq!(router.route(request(~"path/to/test1/no/test3/again")), Some(~"test 3"));
		assert_eq!(router.route(request(~"path/to")), None);
	}

	#[test]
	fn route_formats() {
		let routes = [
			("/", test_1),
			("/path/to/test/no2", test_2),
			("path/to/test3/", test_3),
			("/path/to/test3/again/", test_3)
		];

		let router = Router::from_vec(routes);

		assert_eq!(router.route(request(~"")), Some(~"test 1"));
		assert_eq!(router.route(request(~"path/to/test/no2/")), Some(~"test 2"));
		assert_eq!(router.route(request(~"path/to/test3")), Some(~"test 3"));
		assert_eq!(router.route(request(~"/path/to/test3/again")), Some(~"test 3"));
		assert_eq!(router.route(request(~"//path/to/test3")), None);
	}

	
	#[bench]
	fn search_speed(b: &mut BenchHarness) {
		let routes = [
			("path/to/test1", test_1),
			("path/to/test/no2", test_1),
			("path/to/test1/no/test3", test_1),
			("path/to/other/test1", test_1),
			("path/to/test/no2/again", test_1),
			("other/path/to/test1/no/test3", test_1),
			("path/to/test1", test_1),
			("path/:a/test/no2", test_1),
			("path/to/:b/:c/:a", test_1),
			("path/to/*", test_1),
			("path/to/*/other", test_1)
		];

		let paths = [
			"path/to/test1",
			"path/to/test/no2",
			"path/to/test1/no/test3",
			"path/to/other/test1",
			"path/to/test/no2/again",
			"other/path/to/test1/no/test3",
			"path/a/test1",
			"path/a/test/no2",
			"path/to/b/c/a",
			"path/to/test1/no",
			"path/to",
			"path/to/test1/nothing/at/all"
		];

		let router = Router::from_vec(routes);
		let mut counter = 0;

		b.iter(|| {
			router.route(request(paths[counter].to_owned()));
			counter = (counter + 1) % paths.len()
		});
	}

	
	#[bench]
	fn wildcard_speed(b: &mut BenchHarness) {
		let routes = [
			("*/to/*/*/a", test_1)
		];

		let paths = [
			"path/to/a",
			"path/to/test/a",
			"path/to/test1/no/a",
			"path/to/other/a",
			"path/to/test/no2/a",
			"other/path/to/test1/no/a",
			"path/a/a",
			"path/a/test/a",
			"path/to/b/c/a",
			"path/to/test1/a",
			"path/a",
			"path/to/test1/nothing/at/all/and/all/and/all/and/a"
		];

		let router = Router::from_vec(routes);
		let mut counter = 0;

		b.iter(|| {
			router.route(request(paths[counter].to_owned()));
			counter = (counter + 1) % paths.len()
		});
	}
}