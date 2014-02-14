use std::hashmap::HashMap;

type HandlerFn = fn(~HashMap<~str, &str>) -> ~str;

pub struct Router {
	handler: Option<HandlerFn>,
	static_routes: HashMap<~str, ~Router>,
	variable_route: Option<~Router>,
	variable_names: ~[~str],
	wildcard_route: Option<~Router>
}

impl Router {
	pub fn new() -> Router {
		Router {
			handler: None,
			static_routes: HashMap::new(),
			variable_route: None,
			variable_names: ~[],
			wildcard_route: None
		}
	}
	pub fn from_vec(routes: &[(&str, HandlerFn)]) -> Router {
		let mut root = Router::new();

		for &(path, handler) in routes.iter() {
			root.insert_handler(path, handler);
		}

		root
	}

	pub fn handler(handler: HandlerFn) -> Router {
		Router {
			handler: Some(handler),
			static_routes: HashMap::new(),
			variable_route: None,
			variable_names: ~[],
			wildcard_route: None
		}
	}

	pub fn insert_handler(&mut self, path: &str, handler: HandlerFn) {
		self.insert_handler_vec(path.split('/').collect::<~[&str]>(), ~[], handler);
	}

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

	fn find_or_insert_router<'a>(&'a mut self, key: &str) -> &'a mut ~Router {
		if key.len() > 0 && key.char_at(0) == ':' {
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

	pub fn route(&self, path: &str) -> Option<~str> {
		self.find(path.split('/').collect::<~[&str]>(), &[])
	}

	pub fn find(&self, path: &[&str], variables: &[&str]) -> Option<~str> {
		match path {
			[piece] => {
				self.match_static(piece, variables, |next, vars| { next.exec(vars) })
			},
			[piece, ..rest] => {
				self.match_static(piece, variables, |next, vars| { next.find(rest, vars) })
			},
			[] => {
				self.exec(variables)
			}
		}
	}

	pub fn exec(&self, variables: &[&str]) -> Option<~str> {
		match self.handler {
			Some(handler) => {
				let mut variable_map = ~HashMap::new();
				for (key, &value) in self.variable_names.iter().zip(variables.iter()) {
					variable_map.insert(key.to_owned(), value);
				}
				Some(handler(variable_map))
			},
			None => None
		}
	}

	fn match_static(&self, key: &str, variables: &[&str], action: |&~Router, &[&str]| -> Option<~str>) -> Option<~str> {
		match self.static_routes.find(&key.to_owned()) {
			Some(next) => {
				match action(next, variables) {
					None => self.match_variable(key, variables, action),
					result => result
				}
			},
			None => {
				self.match_variable(key, variables, action)
			}
		}
	}

	fn match_variable(&self, key: &str, variables: &[&str], action: |&~Router, &[&str]| -> Option<~str>) -> Option<~str> {
		match self.variable_route {
			Some(ref next) => {
				let mut new_variables = variables.to_owned();
				new_variables.push(key.clone());
				action(next, new_variables)
			},
			None => None //TODO: Wildcard
		}
	}
}



#[cfg(test)]
mod test {
	use extra::test::BenchHarness;
	use std::hashmap::HashMap;
	use super::Router;

	fn test_1(_: ~HashMap<~str, &str>) -> ~str {
		~"test 1"
	}

	fn test_2(_: ~HashMap<~str, &str>) -> ~str {
		~"test 2"
	}

	fn test_3(_: ~HashMap<~str, &str>) -> ~str {
		~"test 3"
	}

	fn test_var(variables: ~HashMap<~str, &str>) -> ~str {
		let keys = ~[~"a", ~"b", ~"c"];
		keys.iter().filter_map(|key| {
			match variables.find(key) {
				Some(value) => Some(value.to_owned()),
				None => None
			}
		}).collect::<~[~str]>().connect(", ")
	}

	#[test]
	fn one_static_route() {
		let routes = [("path/to/test1", test_1)];

		let router = Router::from_vec(routes);

		assert_eq!(router.route("path/to/test1"), Some(~"test 1"));
		assert_eq!(router.route("path/to"), None);
		assert_eq!(router.route("path/to/test1/nothing"), None);
	}

	#[test]
	fn several_static_routes() {
		let routes = [
			("path/to/test1", test_1),
			("path/to/test/no2", test_2),
			("path/to/test1/no/test3", test_3)
		];

		let router = Router::from_vec(routes);

		assert_eq!(router.route("path/to/test1"), Some(~"test 1"));
		assert_eq!(router.route("path/to/test/no2"), Some(~"test 2"));
		assert_eq!(router.route("path/to/test1/no/test3"), Some(~"test 3"));
		assert_eq!(router.route("path/to/test1/no"), None);
	}

	#[test]
	fn one_variable_route() {
		let routes = [("path/:a/test1", test_var)];

		let router = Router::from_vec(routes);

		assert_eq!(router.route("path/to/test1"), Some(~"to"));
		assert_eq!(router.route("path/to"), None);
		assert_eq!(router.route("path/to/test1/nothing"), None);
	}

	#[test]
	fn several_variable_routes() {
		let routes = [
			("path/to/test1", test_var),
			("path/:a/test/no2", test_var),
			("path/to/:b/:c/:a", test_var)
		];

		let router = Router::from_vec(routes);

		assert_eq!(router.route("path/to/test1"), Some(~""));
		assert_eq!(router.route("path/to/test/no2"), Some(~"to"));
		assert_eq!(router.route("path/to/test1/no/test3"), Some(~"test3, test1, no"));
		assert_eq!(router.route("path/to/test1/no"), None);
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
			("path/to/:b/:c/:a", test_1)
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
			router.route(paths[counter]);
			counter = (counter + 1) % paths.len()
		});
	}
}