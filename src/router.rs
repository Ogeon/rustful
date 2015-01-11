//!`TreeRouter` stores items, such as request handlers, using an HTTP method and a path as keys.
//!
//!The `TreeRouter` can be created from a vector of predefined paths, like this:
//!
//!```ignore
//!let routes = vec![
//!	(Get, "/about", about_us),
//!	(Get, "/user/:user", show_user),
//!	(Get, "/product/:name", show_product),
//!	(Get, "/*", show_error),
//!	(Get, "/", show_welcome)
//!];
//!
//!let router = TreeRouter::from_routes(routes);
//!```
//!
//!Routes may also be added after the `TreeRouter` was created, like this:
//!
//!```ignore
//!let mut router = TreeRouter::new();
//!
//!router.insert(Get, "/about", about_us);
//!router.insert(Get, "/user/:user", show_user);
//!router.insert(Get, "/product/:name", show_product);
//!router.insert(Get, "/*", show_error);
//!router.insert(Get, "/", show_welcome);
//!```
//!
//!TreeRouter can also be used with a special macro for creating routes, called `insert_routes!{...}`:
//!
//!```ignore
//!#![feature(phase)]
//!#[phase(plugin)]
//!extern crate rustful_macros;
//!
//!extern crate rustful;
//!
//!...
//!
//!
//!let router = insert_routes!{
//!	TreeRouter::new(): {
//!		"/about" => Get: about_us,
//!		"/user/:user" => Get: show_user,
//!		"/product/:name" => Get: show_product,
//!		"/*" => Get: show_error,
//!		"/" => Get: show_welcome
//!	}
//!};
//!```
//!
//!This macro will generate the same code as the example above,
//!but it allows more advanced structures to be defined without
//!the need to write the same paths multiple times. This can be
//!useful to lower the risk of typing errors, among other things.
//!
//!This can be found in the crate `rustful_macros`.

use Router;

use std::collections::HashMap;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::vec::Vec;
use std::borrow::ToOwned;
use hyper::method::Method;

use self::Branch::{Static, Variable, Wildcard};

pub type RouterResult<'a, T> = Option<(&'a T, HashMap<String, String>)>;

enum Branch {
	Static,
	Variable,
	Wildcard
}

//impl_clonable_fn!(<>, <[A], [B]>, <[A], [B], [C]>)

///Stores items, such as request handlers, using an HTTP method and a path as keys.
///
///Paths can be static (`"path/to/item"`) or variable (`"users/:group/:user"`)
///and contain wildcards (`"path/*/item/*"`).
///
///Variables (starting with `:`) will match whatever word the request path contains at
///that point and it will be sent as a value to the item.
///
///Wildcards (a single `*`) will consume the segments until the rest of the
///path gives a match.
///
///```ignore
///pattern = "a/*/b"
///"a/c/b" -> match
///"a/c/d/b" -> match
///"a/b" -> no match
///"a/c/b/d" -> no match
///```
///
///```ignore
///pattern = "a/b/*"
///"a/b/c" -> match
///"a/b/c/d" -> match
///"a/b" -> no match
///```
#[derive(Clone)]
pub struct TreeRouter<T> {
	items: HashMap<String, (T, Vec<String>)>,
	static_routes: HashMap<String, TreeRouter<T>>,
	variable_route: Option<Box<TreeRouter<T>>>,
	wildcard_route: Option<Box<TreeRouter<T>>>
}

impl<T> TreeRouter<T> {
	///Creates an empty `TreeRouter`.
	pub fn new() -> TreeRouter<T> {
		TreeRouter {
			items: HashMap::new(),
			static_routes: HashMap::new(),
			variable_route: None,
			wildcard_route: None
		}
	}

	///Generates a `TreeRouter` tree from a set of items and paths.
	pub fn from_routes(routes: Vec<(Method, &str, T)>) -> TreeRouter<T> {
		let mut root = TreeRouter::new();

		for (method, path, item) in routes.into_iter() {
			root.insert(method, path, item);
		}

		root
	}

	//Tries to find a router matching the key or inserts a new one if none exists
	fn find_or_insert_router<'a>(&'a mut self, key: &str) -> &'a mut TreeRouter<T> {
		if key == "*" {
			if self.wildcard_route.is_none() {
				self.wildcard_route = Some(Box::new(TreeRouter::new()));
			}
			&mut **self.wildcard_route.as_mut::<'a>().unwrap()
		} else if key.len() > 0 && key.char_at(0) == ':' {
			if self.variable_route.is_none() {
				self.variable_route = Some(Box::new(TreeRouter::new()));
			}
			&mut **self.variable_route.as_mut::<'a>().unwrap()
		} else {
			match self.static_routes.entry(key.to_string()) {
				Occupied(entry) => entry.into_mut(),
				Vacant(entry) => entry.insert(TreeRouter::new())
			}
		}
	}
}

impl<T> Router for TreeRouter<T> {
	type Handler = T;

	fn find<'a>(&'a self, method: &Method, path: &str) -> RouterResult<'a, T> {
		let path = path_to_vec(path);
		let method_str = method.to_string();

		if path.len() == 0 {
			match self.items.get(&method_str) {
				Some(&(ref item, _)) => Some((item, HashMap::new())),
				None => None
			}
		} else {
			let mut variables: Vec<_> = ::std::iter::repeat(false).take(path.len()).collect();

			let mut stack = vec![(self, Wildcard, 0), (self, Variable, 0), (self, Static, 0)];

			while stack.len() > 0 {
				let (current, branch, index) = stack.pop().unwrap();

				if index == path.len() {
					match current.items.get(&method_str) {
						Some(&(ref item, ref variable_names)) => {
							let values = path.into_iter().zip(variables.into_iter()).filter_map(|(v, keep)| {
								if keep {
									Some(v)
								} else {
									None
								}
							});

							let var_map = variable_names.iter().zip(values).map(|(key, value)| {
								(key.clone(), value)
							});

							return Some((item, var_map.collect()))
						},
						None => continue
					}
				}

				match branch {
					Static => {
						current.static_routes.get(&path[index]).map(|next| {
							variables.get_mut(index).map(|v| *v = false);
							
							stack.push((&*next, Wildcard, index+1));
							stack.push((&*next, Variable, index+1));
							stack.push((&*next, Static, index+1));
						});
					},
					Variable => {
						current.variable_route.as_ref().map(|next| {
							variables.get_mut(index).map(|v| *v = true);

							stack.push((&**next, Wildcard, index+1));
							stack.push((&**next, Variable, index+1));
							stack.push((&**next, Static, index+1));
						});
					},
					Wildcard => {
						current.wildcard_route.as_ref().map(|next| {
							variables.get_mut(index).map(|v| *v = false);

							stack.push((current, Wildcard, index+1));
							stack.push((&**next, Wildcard, index+1));
							stack.push((&**next, Variable, index+1));
							stack.push((&**next, Static, index+1));
						});
					}
				}
			}

			None
		}
	}

	///Inserts an item into the `TreeRouter` at a given path.
	fn insert(&mut self, method: Method, path: &str, item: T) {
		let path = path_to_vec(path.trim());

		if path.len() == 0 {
			self.items.insert(method.to_string().to_owned(), (item, Vec::new()));
		} else {
			let (endpoint, variable_names) = path.into_iter().fold((self, Vec::new()),

				|(current, mut variable_names), piece| {
					let next = current.find_or_insert_router(piece.as_slice());
					if piece.len() > 0 && piece.as_slice().char_at(0) == ':' {
						//piece.shift_char();
						variable_names.push(piece.slice_from(1).to_owned());
					}

					(next, variable_names)
				}

			);

			endpoint.items.insert(method.to_string().to_owned(), (item, variable_names));
		}
	}
}

impl<T: Clone> TreeRouter<T> {
	///Insert an other TreeRouter at a path. The content of the other TreeRouter will be merged with this one.
	///Content with the same path and method will be overwritten.
	pub fn insert_router(&mut self, path: &str, router: &TreeRouter<T>) {
		let path = path_to_vec(path.trim());

		if path.len() == 0 {
			self.merge_router(Vec::new(), router);
		} else {
			let (endpoint, variable_names) = path.into_iter().fold((self, Vec::new()),

				|(current, mut variable_names), piece| {
					let next = current.find_or_insert_router(piece.as_slice());
					if piece.len() > 0 && piece.as_slice().char_at(0) == ':' {
						//piece.shift_char();
						variable_names.push(piece.slice_from(1).to_owned());
					}

					(next, variable_names)
				}

			);

			endpoint.merge_router(variable_names, router);
		}
	}

	//Mergers this TreeRouter with an other TreeRouter.
	fn merge_router(&mut self, variable_names: Vec<String>, router: &TreeRouter<T>) {
		for (key, &(ref item, ref var_names)) in router.items.iter() {
			let mut new_var_names = variable_names.clone();
			new_var_names.push_all(var_names.as_slice());
			self.items.insert(key.clone(), (item.clone(), new_var_names));
		}

		for (key, router) in router.static_routes.iter() {
			let next = match self.static_routes.entry(key.clone()) {
				Occupied(entry) => entry.into_mut(),
				Vacant(entry) => entry.insert(TreeRouter::new())
			};
			next.merge_router(variable_names.clone(), router);
		}

		if router.variable_route.is_some() {
			if self.variable_route.is_none() {
				self.variable_route = Some(Box::new(TreeRouter::new()));
			}

			match self.variable_route.as_mut() {
				Some(next) => next.merge_router(variable_names.clone(), &**router.variable_route.as_ref().unwrap()),
				None => {}
			}
		}

		if router.wildcard_route.is_some() {
			if self.wildcard_route.is_none() {
				self.wildcard_route = Some(Box::new(TreeRouter::new()));
			}

			match self.wildcard_route.as_mut() {
				Some(next) => next.merge_router(variable_names.clone(), &**router.wildcard_route.as_ref().unwrap()),
				None => {}
			}
		}
	}
}

//Converts a path to a suitable array of path segments
fn path_to_vec(path: &str) -> Vec<String> {
	if path.len() == 0 {
		vec!()
	} else if path.len() == 1 {
		if path == "/" {
			vec!()
		} else {
			vec!(path.to_string())
		}
	} else {
		let start = if path.char_at(0) == '/' { 1 } else { 0 };
		let end = if path.char_at(path.len() - 1) == '/' { 1 } else { 0 };
		path.slice(start, path.len() - end).split('/').map(|s| s.to_string()).collect()
	}
}


#[cfg(test)]
mod test {
	use Router;
	use test::Bencher;
	use super::TreeRouter;
	use hyper::method::Method::{Get, Post, Delete, Put, Head};
	use std::collections::HashMap;
	use std::vec::Vec;

	fn check_variable(result: Option<(& &'static str, HashMap<String, String>)>, expected: Option<&str>) {
		assert!(match result {
			Some((_, mut variables)) => match expected {
				Some(expected) => {
					let keys = vec!("a", "b", "c");
					let result = keys.into_iter().filter_map(|key| {
						match variables.remove(key) {
							Some(value) => Some(value),
							None => None
						}
					}).collect::<Vec<String>>().connect(", ");

					expected.to_string() == result
				},
				None => false
			},
			None => expected.is_none()
		});
	}

	fn check(result: Option<(& &'static str, HashMap<String, String>)>, expected: Option<&str>) {
		assert!(match result {
			Some((result, _)) => match expected {
				Some(expected) => result.to_string() == expected.to_string(),
				None => false
			},
			None => expected.is_none()
		});
	}

	#[test]
	fn one_static_route() {
		let routes = vec![(Get, "path/to/test1", "test 1")];

		let router = TreeRouter::from_routes(routes);

		check(router.find(&Get, "path/to/test1"), Some("test 1"));
		check(router.find(&Get, "path/to"), None);
		check(router.find(&Get, "path/to/test1/nothing"), None);
	}

	#[test]
	fn several_static_routes() {
		let routes = vec![
			(Get, "", "test 1"),
			(Get, "path/to/test/no2", "test 2"),
			(Get, "path/to/test1/no/test3", "test 3")
		];

		let router = TreeRouter::from_routes(routes);

		check(router.find(&Get, ""), Some("test 1"));
		check(router.find(&Get, "path/to/test/no2"), Some("test 2"));
		check(router.find(&Get, "path/to/test1/no/test3"), Some("test 3"));
		check(router.find(&Get, "path/to/test1/no"), None);
	}

	#[test]
	fn one_variable_route() {
		let routes = vec![(Get, "path/:a/test1", "test_var")];

		let router = TreeRouter::from_routes(routes);

		check_variable(router.find(&Get, "path/to/test1"), Some("to"));
		check_variable(router.find(&Get, "path/to"), None);
		check_variable(router.find(&Get, "path/to/test1/nothing"), None);
	}

	#[test]
	fn several_variable_routes() {
		let routes = vec![
			(Get, "path/to/test1", "test_var"),
			(Get, "path/:a/test/no2", "test_var"),
			(Get, "path/to/:b/:c/:a", "test_var"),
			(Post, "path/to/:c/:a/:b", "test_var")
		];

		let router = TreeRouter::from_routes(routes);

		check_variable(router.find(&Get, "path/to/test1"), Some(""));
		check_variable(router.find(&Get, "path/to/test/no2"), Some("to"));
		check_variable(router.find(&Get, "path/to/test1/no/test3"), Some("test3, test1, no"));
		check_variable(router.find(&Post, "path/to/test1/no/test3"), Some("no, test3, test1"));
		check_variable(router.find(&Get, "path/to/test1/no"), None);
	}

	#[test]
	fn one_wildcard_end_route() {
		let routes = vec![(Get, "path/to/*", "test 1")];

		let router = TreeRouter::from_routes(routes);

		check(router.find(&Get, "path/to/test1"), Some("test 1"));
		check(router.find(&Get, "path/to/same/test1"), Some("test 1"));
		check(router.find(&Get, "path/to/the/same/test1"), Some("test 1"));
		check(router.find(&Get, "path/to"), None);
		check(router.find(&Get, "path"), None);
	}


	#[test]
	fn one_wildcard_middle_route() {
		let routes = vec![(Get, "path/*/test1", "test 1")];

		let router = TreeRouter::from_routes(routes);

		check(router.find(&Get, "path/to/test1"), Some("test 1"));
		check(router.find(&Get, "path/to/same/test1"), Some("test 1"));
		check(router.find(&Get, "path/to/the/same/test1"), Some("test 1"));
		check(router.find(&Get, "path/to"), None);
		check(router.find(&Get, "path"), None);
	}

	#[test]
	fn one_universal_wildcard_route() {
		let routes = vec![(Get, "*", "test 1")];

		let router = TreeRouter::from_routes(routes);

		check(router.find(&Get, "path/to/test1"), Some("test 1"));
		check(router.find(&Get, "path/to/same/test1"), Some("test 1"));
		check(router.find(&Get, "path/to/the/same/test1"), Some("test 1"));
		check(router.find(&Get, "path/to"), Some("test 1"));
		check(router.find(&Get, "path"), Some("test 1"));
		check(router.find(&Get, ""), None);
	}

	#[test]
	fn several_wildcards_routes() {
		let routes = vec![
			(Get, "path/to/*", "test 1"),
			(Get, "path/*/test/no2", "test 2"),
			(Get, "path/to/*/*/*", "test 3")
		];

		let router = TreeRouter::from_routes(routes);

		check(router.find(&Get, "path/to/test1"), Some("test 1"));
		check(router.find(&Get, "path/for/test/no2"), Some("test 2"));
		check(router.find(&Get, "path/to/test1/no/test3"), Some("test 3"));
		check(router.find(&Get, "path/to/test1/no/test3/again"), Some("test 3"));
		check(router.find(&Get, "path/to"), None);
	}

	#[test]
	fn route_formats() {
		let routes = vec![
			(Get, "/", "test 1"),
			(Get, "/path/to/test/no2", "test 2"),
			(Get, "path/to/test3/", "test 3"),
			(Get, "/path/to/test3/again/", "test 3")
		];

		let router = TreeRouter::from_routes(routes);

		check(router.find(&Get, ""), Some("test 1"));
		check(router.find(&Get, "path/to/test/no2/"), Some("test 2"));
		check(router.find(&Get, "path/to/test3"), Some("test 3"));
		check(router.find(&Get, "/path/to/test3/again"), Some("test 3"));
		check(router.find(&Get, "//path/to/test3"), None);
	}

	#[test]
	fn http_methods() {
		let routes = vec![
			(Get, "/", "get"),
			(Post, "/", "post"),
			(Delete, "/", "delete"),
			(Put, "/", "put")
		];

		let router = TreeRouter::from_routes(routes);


		check(router.find(&Get, "/"), Some("get"));
		check(router.find(&Post, "/"), Some("post"));
		check(router.find(&Delete, "/"), Some("delete"));
		check(router.find(&Put, "/"), Some("put"));
		check(router.find(&Head, "/"), None);
	}

	#[test]
	fn merge_routers() {
		let routes1 = vec![
			(Get, "", "test 1"),
			(Get, "path/to/test/no2", "test 2"),
			(Get, "path/to/test1/no/test3", "test 3")
		];

		let routes2 = vec![
			(Get, "", "test 1"),
			(Get, "test/no5", "test 5"),
			(Post, "test1/no/test3", "test 3 post")
		];

		let mut router1 = TreeRouter::from_routes(routes1);
		let router2 = TreeRouter::from_routes(routes2);

		router1.insert_router("path/to", &router2);

		check(router1.find(&Get, ""), Some("test 1"));
		check(router1.find(&Get, "path/to/test/no2"), Some("test 2"));
		check(router1.find(&Get, "path/to/test1/no/test3"), Some("test 3"));
		check(router1.find(&Post, "path/to/test1/no/test3"), Some("test 3 post"));
		check(router1.find(&Get, "path/to/test/no5"), Some("test 5"));
		check(router1.find(&Get, "path/to"), Some("test 1"));
	}


	#[test]
	fn merge_routers_variables() {
		let routes1 = vec![(Get, ":a/:b/:c", "test 2")];
		let routes2 = vec![(Get, ":b/:c/test", "test 1")];

		let mut router1 = TreeRouter::from_routes(routes1);
		let router2 = TreeRouter::from_routes(routes2);

		router1.insert_router(":a", &router2);
		
		check_variable(router1.find(&Get, "path/to/test1"), Some("path, to, test1"));
		check_variable(router1.find(&Get, "path/to/test1/test"), Some("path, to, test1"));
	}


	#[test]
	fn merge_routers_wildcard() {
		let routes1 = vec![(Get, "path/to", "test 2")];
		let routes2 = vec![(Get, "*/test1", "test 1")];

		let mut router1 = TreeRouter::from_routes(routes1);
		let router2 = TreeRouter::from_routes(routes2);

		router1.insert_router("path", &router2);

		check(router1.find(&Get, "path/to/test1"), Some("test 1"));
		check(router1.find(&Get, "path/to/same/test1"), Some("test 1"));
		check(router1.find(&Get, "path/to/the/same/test1"), Some("test 1"));
		check(router1.find(&Get, "path/to"), Some("test 2"));
		check(router1.find(&Get, "path"), None);
	}

	
	#[bench]
	fn search_speed(b: &mut Bencher) {
		let routes = vec![
			(Get, "path/to/test1", "test 1"),
			(Get, "path/to/test/no2", "test 1"),
			(Get, "path/to/test1/no/test3", "test 1"),
			(Get, "path/to/other/test1", "test 1"),
			(Get, "path/to/test/no2/again", "test 1"),
			(Get, "other/path/to/test1/no/test3", "test 1"),
			(Get, "path/to/test1", "test 1"),
			(Get, "path/:a/test/no2", "test 1"),
			(Get, "path/to/:b/:c/:a", "test 1"),
			(Get, "path/to/*", "test 1"),
			(Get, "path/to/*/other", "test 1")
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

		let router = TreeRouter::from_routes(routes);
		let mut counter = 0;

		b.iter(|| {
			router.find(&Get, paths[counter]);
			counter = (counter + 1) % paths.len()
		});
	}

	
	#[bench]
	fn wildcard_speed(b: &mut Bencher) {
		let routes = vec![
			(Get, "*/to/*/*/a", "test 1")
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

		let router = TreeRouter::from_routes(routes);
		let mut counter = 0;

		b.iter(|| {
			router.find(&Get, paths[counter]);
			counter = (counter + 1) % paths.len()
		});
	}
}