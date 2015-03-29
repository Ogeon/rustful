//!Routers stores request handlers, using an HTTP method and a path as keys.
//!
//!Rustful provides a tree structured all-round router called `TreeRouter`,
//!but any other type of router can be used, as long as it implements the
//!`Router` trait. This will also make it possible to initialize it using the
//!`insert_routes!` macro:
//!
//!```
//!#[macro_use]
//!extern crate rustful;
//!use rustful::Method::Get;
//!use rustful::TreeRouter;
//!# use rustful::{Handler, Context, Response};
//!
//!# #[derive(Copy)]
//!# struct DummyHandler;
//!# impl Handler for DummyHandler {
//!#     fn handle_request(&self, _: Context, _: Response){}
//!# }
//!# fn main() {
//!# let about_us = DummyHandler;
//!# let show_user = DummyHandler;
//!# let list_users = DummyHandler;
//!# let show_product = DummyHandler;
//!# let list_products = DummyHandler;
//!# let show_error = DummyHandler;
//!# let show_welcome = DummyHandler;
//!let router = insert_routes!{
//!    TreeRouter::new() => {
//!        "/about" => Get: about_us,
//!        "/users" => {
//!            "/" => Get: list_users,
//!            ":id" => Get: show_user
//!        },
//!        "/products" => {
//!            "/" => Get: list_products,
//!            ":id" => Get: show_product
//!        },
//!        "/*" => Get: show_error,
//!        "/" => Get: show_welcome
//!    }
//!};
//!# }
//!```
//!
//!This macro creates the same structure as the example below, but it allows
//!tree structures to be defined without the need to write the same paths
//!multiple times. This can be useful to lower the risk of typing errors,
//!among other things.
//!
//!Routes may also be added using the insert method, like this:
//!
//!```
//!extern crate rustful;
//!use rustful::Method::Get;
//!use rustful::{Router, TreeRouter};
//!# use rustful::{Handler, Context, Response};
//!
//!# #[derive(Copy)]
//!# struct DummyHandler;
//!# impl Handler for DummyHandler {
//!#     fn handle_request(&self, _: Context, _: Response){}
//!# }
//!# fn main() {
//!# let about_us = DummyHandler;
//!# let show_user = DummyHandler;
//!# let list_users = DummyHandler;
//!# let show_product = DummyHandler;
//!# let list_products = DummyHandler;
//!# let show_error = DummyHandler;
//!# let show_welcome = DummyHandler;
//!let mut router = TreeRouter::new();
//!
//!router.insert(Get, "/about", about_us);
//!router.insert(Get, "/users", list_users);
//!router.insert(Get, "/users/:id", show_user);
//!router.insert(Get, "/products", list_products);
//!router.insert(Get, "/products/:id", show_product);
//!router.insert(Get, "/*", show_error);
//!router.insert(Get, "/", show_welcome);
//!# }
//!```

#![stable]

use std::collections::HashMap;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::vec::Vec;
use std::borrow::ToOwned;
use std::iter::{Iterator, IntoIterator, FromIterator, FlatMap};
use std::str::SplitTerminator;
use std::slice::Iter;
use std::ops::Deref;
use hyper::method::Method;

use handler::Handler;

use self::Branch::{Static, Variable, Wildcard};

pub type RouterResult<'a, T> = Option<(&'a T, HashMap<String, String>)>;

///A common trait for routers.
///
///A router must to implement this trait to be usable in a Rustful server. This
///trait will also make the router compatible with the `insert_routes!` macro.
pub trait Router {
    type Handler;

    ///Insert a new handler into the router.
    fn insert<'r, R: Route<'r> + ?Sized>(&mut self, method: Method, route: &'r R, handler: Self::Handler);

    ///Find and return the matching handler and variable values.
    fn find<'a: 'r, 'r, R: Route<'r> + ?Sized>(&'a self, method: &Method, route: &'r R) -> RouterResult<'a, Self::Handler>;
}

impl<H: Handler> Router for H {
    type Handler = H;

    fn find<'a: 'r, 'r, R: Route<'r> + ?Sized>(&'a self, _method: &Method, _route: &'r R) -> RouterResult<'a, H> {
        Some((self, HashMap::new()))
    }

    fn insert<'r, R: Route<'r> + 'r + ?Sized>(&mut self, _method: Method, _route: &'r R, _handler: H) {}
}

///A segmented route.
pub trait Route<'a> {
    type Segments: Iterator<Item=&'a str>;

    ///Create a route segment iterator. The iterator is expected to return
    ///None for a root path (`/`).
    ///
    ///```rust
    ///# use rustful::router::Route;
    ///let root = "/";
    ///assert_eq!(root.segments().next(), None);
    ///
    ///let path = ["/path", "to/somewhere/", "/", "/else/"];
    ///let segments = path.segments().collect();
    ///assert_eq!(segments, vec!["path", "to", "somewhere", "else"]);
    ///```
    fn segments(&'a self) -> <Self as Route<'a>>::Segments;
}

impl<'a> Route<'a> for str {
    type Segments = RouteIter<SplitTerminator<'a, char>>;

    fn segments(&'a self) -> <Self as Route<'a>>::Segments {
        let s = if self.starts_with('/') {
            &self[1..]
        } else {
            self
        };

        if s.len() == 0 {
            RouteIter::Root
        } else {
            RouteIter::Path(s.split_terminator('/'))
        }
    }
}


impl<'a, R: Route<'a>> Route<'a> for [R] {
    type Segments = FlatMap<Iter<'a, R>, <R as Route<'a>>::Segments, fn(&'a R) -> <R as Route<'a>>::Segments>;

    fn segments(&'a self) -> <Self as Route>::Segments {
        fn segments<'a, T: Route<'a> + 'a>(s: &'a T) -> <T as Route<'a>>::Segments {
            s.segments()
        }

        self.iter().flat_map(segments::<'a>)
    }
}

impl<'a, 'b: 'a, T: Deref<Target=R> + 'b, R: Route<'a> + ?Sized + 'b> Route<'a> for T {
    type Segments = <R as Route<'a>>::Segments;

    #[inline]
    fn segments(&'a self) -> <Self as Route<'a>>::Segments {
        self.deref().segments()
    }
}

///Utility iterator for when a root path may be hard to represent.
pub enum RouteIter<I: Iterator> {
    ///A root path (`/`).
    Root,
    ///A non-root path (`path/to/somewhere`).
    Path(I)
}

impl<I: Iterator> Iterator for RouteIter<I> {
    type Item = <I as Iterator>::Item;

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        match self {
            &mut RouteIter::Path(ref mut i) => i.next(),
            &mut RouteIter::Root => None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            &RouteIter::Path(ref i) => i.size_hint(),
            &RouteIter::Root => (0, Some(0))
        }
    }
}

enum Branch {
    Static,
    Variable,
    Wildcard
}

///Stores items, such as request handlers, using an HTTP method and a path as keys.
///
///Paths can be static (`"path/to/item"`) or variable (`"users/:group/:user"`)
///and contain wildcards (`"path/*/item/*"`). Variables (starting with `:`)
///will match whatever word the request path contains at that point and it
///will be sent as a value to the item. Wildcards (a single `*`) will consume
///the segments until the rest of the path gives a match.
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
#[stable]
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

    //Tries to find a router matching the key or inserts a new one if none exists.
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

    fn find<'a: 'r, 'r, R: Route<'r> + ?Sized>(&'a self, method: &Method, route: &'r R) -> RouterResult<'a, T> {
        let path = route.segments().collect::<Vec<_>>();
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
                                (key.clone(), value.to_owned())
                            });

                            return Some((item, var_map.collect()))
                        },
                        None => continue
                    }
                }

                match branch {
                    Static => {
                        current.static_routes.get(path[index]).map(|next| {
                            variables.get_mut(index).map(|v| *v = false);
                            
                            stack.push((next, Wildcard, index+1));
                            stack.push((next, Variable, index+1));
                            stack.push((next, Static, index+1));
                        });
                    },
                    Variable => {
                        current.variable_route.as_ref().map(|next| {
                            variables.get_mut(index).map(|v| *v = true);

                            stack.push((next, Wildcard, index+1));
                            stack.push((next, Variable, index+1));
                            stack.push((next, Static, index+1));
                        });
                    },
                    Wildcard => {
                        current.wildcard_route.as_ref().map(|next| {
                            variables.get_mut(index).map(|v| *v = false);

                            stack.push((current, Wildcard, index+1));
                            stack.push((next, Wildcard, index+1));
                            stack.push((next, Variable, index+1));
                            stack.push((next, Static, index+1));
                        });
                    }
                }
            }

            None
        }
    }

    fn insert<'r, R: Route<'r> + ?Sized>(&mut self, method: Method, route: &'r R, item: T) {
        let (endpoint, variable_names) = route.segments().fold((self, Vec::new()),

            |(current, mut variable_names), piece| {
                let next = current.find_or_insert_router(&piece);
                if piece.len() > 0 && piece.char_at(0) == ':' {
                    variable_names.push(piece[1..].to_owned());
                }

                (next, variable_names)
            }

        );

        endpoint.items.insert(method.to_string().to_owned(), (item, variable_names));
    }
}

impl<T: Clone> TreeRouter<T> {
    ///Insert an other TreeRouter at a path. The content of the other TreeRouter will be merged with this one.
    ///Content with the same path and method will be overwritten.
    pub fn insert_router<'r, R: Route<'r> + ?Sized>(&mut self, route: &'r R, router: &TreeRouter<T>) {
        let (endpoint, variable_names) = route.segments().fold((self, Vec::new()),

            |(current, mut variable_names), piece| {
                let next = current.find_or_insert_router(&piece);
                if piece.len() > 0 && piece.char_at(0) == ':' {
                    variable_names.push(piece[1..].to_owned());
                }

                (next, variable_names)
            }

        );

        endpoint.merge_router(variable_names, router);
    }

    //Mergers this TreeRouter with an other TreeRouter.
    fn merge_router(&mut self, variable_names: Vec<String>, router: &TreeRouter<T>) {
        for (key, &(ref item, ref var_names)) in router.items.iter() {
            let mut new_var_names = variable_names.clone();
            new_var_names.push_all(var_names);
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
                Some(next) => next.merge_router(variable_names.clone(), router.variable_route.as_ref().unwrap()),
                None => {}
            }
        }

        if router.wildcard_route.is_some() {
            if self.wildcard_route.is_none() {
                self.wildcard_route = Some(Box::new(TreeRouter::new()));
            }

            match self.wildcard_route.as_mut() {
                Some(next) => next.merge_router(variable_names.clone(), router.wildcard_route.as_ref().unwrap()),
                None => {}
            }
        }
    }
}

impl<'r, T, R: Route<'r> + 'r + ?Sized> FromIterator<(Method, &'r R, T)> for TreeRouter<T> {
    ///Create a `TreeRouter` from a collection of routes.
    ///
    ///```
    ///extern crate rustful;
    ///use rustful::Method::Get;
    ///use rustful::TreeRouter;
    ///# use rustful::{Handler, Context, Response};
    ///
    ///# #[derive(Copy)]
    ///# struct DummyHandler;
    ///# impl Handler for DummyHandler {
    ///#     fn handle_request(&self, _: Context, _: Response){}
    ///# }
    ///# fn main() {
    ///# let about_us = DummyHandler;
    ///# let show_user = DummyHandler;
    ///# let list_users = DummyHandler;
    ///# let show_product = DummyHandler;
    ///# let list_products = DummyHandler;
    ///# let show_error = DummyHandler;
    ///# let show_welcome = DummyHandler;
    ///let routes = vec![
    ///    (Get, "/about", about_us),
    ///    (Get, "/users", list_users),
    ///    (Get, "/users/:id", show_user),
    ///    (Get, "/products", list_products),
    ///    (Get, "/products/:id", show_product),
    ///    (Get, "/*", show_error),
    ///    (Get, "/", show_welcome)
    ///];
    ///
    ///let router: TreeRouter<_> = routes.into_iter().collect();
    ///# }
    ///```
    fn from_iter<I: IntoIterator<Item=(Method, &'r R, T)>>(iterator: I) -> TreeRouter<T> {
        let mut root = TreeRouter::new();

        for (method, route, item) in iterator {
            root.insert(method, route, item);
        }

        root
    }
}


#[cfg(test)]
mod test {
    use router::Router;
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

        let router = routes.into_iter().collect::<TreeRouter<_>>();

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

        let router = routes.into_iter().collect::<TreeRouter<_>>();

        check(router.find(&Get, ""), Some("test 1"));
        check(router.find(&Get, "path/to/test/no2"), Some("test 2"));
        check(router.find(&Get, "path/to/test1/no/test3"), Some("test 3"));
        check(router.find(&Get, "path/to/test1/no"), None);
    }

    #[test]
    fn one_variable_route() {
        let routes = vec![(Get, "path/:a/test1", "test_var")];

        let router = routes.into_iter().collect::<TreeRouter<_>>();

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

        let router = routes.into_iter().collect::<TreeRouter<_>>();

        check_variable(router.find(&Get, "path/to/test1"), Some(""));
        check_variable(router.find(&Get, "path/to/test/no2"), Some("to"));
        check_variable(router.find(&Get, "path/to/test1/no/test3"), Some("test3, test1, no"));
        check_variable(router.find(&Post, "path/to/test1/no/test3"), Some("no, test3, test1"));
        check_variable(router.find(&Get, "path/to/test1/no"), None);
    }

    #[test]
    fn one_wildcard_end_route() {
        let routes = vec![(Get, "path/to/*", "test 1")];

        let router = routes.into_iter().collect::<TreeRouter<_>>();

        check(router.find(&Get, "path/to/test1"), Some("test 1"));
        check(router.find(&Get, "path/to/same/test1"), Some("test 1"));
        check(router.find(&Get, "path/to/the/same/test1"), Some("test 1"));
        check(router.find(&Get, "path/to"), None);
        check(router.find(&Get, "path"), None);
    }


    #[test]
    fn one_wildcard_middle_route() {
        let routes = vec![(Get, "path/*/test1", "test 1")];

        let router = routes.into_iter().collect::<TreeRouter<_>>();

        check(router.find(&Get, "path/to/test1"), Some("test 1"));
        check(router.find(&Get, "path/to/same/test1"), Some("test 1"));
        check(router.find(&Get, "path/to/the/same/test1"), Some("test 1"));
        check(router.find(&Get, "path/to"), None);
        check(router.find(&Get, "path"), None);
    }

    #[test]
    fn one_universal_wildcard_route() {
        let routes = vec![(Get, "*", "test 1")];

        let router = routes.into_iter().collect::<TreeRouter<_>>();

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

        let router = routes.into_iter().collect::<TreeRouter<_>>();

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

        let router = routes.into_iter().collect::<TreeRouter<_>>();

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

        let router = routes.into_iter().collect::<TreeRouter<_>>();


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

        let mut router1 = routes1.into_iter().collect::<TreeRouter<_>>();
        let router2 = routes2.into_iter().collect::<TreeRouter<_>>();

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

        let mut router1 = routes1.into_iter().collect::<TreeRouter<_>>();
        let router2 = routes2.into_iter().collect::<TreeRouter<_>>();

        router1.insert_router(":a", &router2);
        
        check_variable(router1.find(&Get, "path/to/test1"), Some("path, to, test1"));
        check_variable(router1.find(&Get, "path/to/test1/test"), Some("path, to, test1"));
    }


    #[test]
    fn merge_routers_wildcard() {
        let routes1 = vec![(Get, "path/to", "test 2")];
        let routes2 = vec![(Get, "*/test1", "test 1")];

        let mut router1 = routes1.into_iter().collect::<TreeRouter<_>>();
        let router2 = routes2.into_iter().collect::<TreeRouter<_>>();

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

        let router = routes.into_iter().collect::<TreeRouter<_>>();
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

        let router = routes.into_iter().collect::<TreeRouter<_>>();
        let mut counter = 0;

        b.iter(|| {
            router.find(&Get, paths[counter]);
            counter = (counter + 1) % paths.len()
        });
    }
}