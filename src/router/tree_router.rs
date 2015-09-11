use std::collections::HashMap;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::borrow::ToOwned;
use std::iter::{Iterator, IntoIterator, FromIterator};
use std::ops::Deref;
use hyper::method::Method;

use router::{Router, Route, Endpoint};
use context::MaybeUtf8Owned;
use context::hypermedia::{Link, LinkSegment, SegmentType};
use handler::Handler;

use self::Branch::{Static, Variable, Wildcard};

#[derive(PartialEq)]
enum Branch {
    Static,
    Variable,
    Wildcard
}

struct VariableIter<'a, I> {
    iter: I,
    names: &'a [MaybeUtf8Owned],
    current: Option<(usize, MaybeUtf8Owned, MaybeUtf8Owned)>
}

impl<'a, I: Iterator<Item=(usize, &'a [u8])>> VariableIter<'a, I> {
    fn new(iter: I, names: &'a [MaybeUtf8Owned]) -> VariableIter<'a, I> {
        VariableIter {
            iter: iter,
            names: names,
            current: None
        }
    }
}

impl<'a, I: Iterator<Item=(usize, &'a [u8])>> Iterator for VariableIter<'a, I> {
    type Item=(MaybeUtf8Owned, MaybeUtf8Owned);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((next_index, next_segment)) = self.iter.next() {
            //validate next_index and check if the variable has a name
            debug_assert!(next_index < self.names.len(), format!("invalid variable name index! variable_names.len(): {}, index: {}", self.names.len(), next_index));
            let next_name = match self.names.get(next_index) {
                None => continue,
                Some(n) if n.is_empty() => continue,
                Some(n) => n
            };

            if let Some((index, name, mut segment)) = self.current.take() {
                if index == next_index {
                    //this is a part of the current sequence
                    segment.push_char('/');
                    segment.push_bytes(next_segment);
                    self.current = Some((index, name, segment));
                } else {
                    //the current sequence has ended
                    self.current = Some((next_index, (*next_name).clone(), next_segment.to_owned().into()));
                    return Some((name, segment));
                }
            } else {
                //this is the first named variable
                self.current = Some((next_index, (*next_name).clone(), next_segment.to_owned().into()));
            }
        }

        //return the last variable
        self.current.take().map(|(_, name, segment)| (name, segment))
    }
}

///Stores handlers, using an HTTP method and a route as key.
///
///#Variables
///
///Routes may contain variables, that are useful for capturing parts of the
///requested path as input to the handler. The syntax for a variable is simply
///an indicator character (`:` or `*`) followed by a label. Variables without
///labels are also valid, but their values will be discarded.
///
///##Variable Segments (:label)
///
///A variable segment will match a single arbitrary segment. They are probably
///the most commonly used variables and may, for example, be used to select a
///blog post: `"posts/:year/:month/:day/:title_slug"`.
///
///```text
///pattern = "a/:v/b"
///"a/c/b" -> v = "c"
///"a/c/d/b" -> no match
///"a/b" -> no match
///"a/c/b/d" -> no match
///```
///
///##Variable Sequences (*label)
///
///A variable sequence is similar to a variable segment, but with the
///difference that it may consume multiple segments until the rest of the path
///gives a match. An example use case is a route for downloadable files that
///may be arranged in arbitrary directories: `"downloads/*file_path"`.
///
///```text
///pattern = "a/*v/b"
///"a/c/b" -> v = "c"
///"a/c/d/b" -> v = "c/d"
///"a/b" -> no match
///"a/c/b/d" -> no match
///```
///
///```text
///pattern = "a/b/*v"
///"a/b/c" -> v = "c"
///"a/b/c/d" -> v = "c/d"
///"a/b" -> no match
///```
///
///#Hyperlinks
///
///The 'TreeRouter` has support for shallow hyperlinks to children, siblings,
///cousins, ans so forth. The use of variable sequences complicates this
///process and may cause confusing results in certain situations. The
///hyperlinks may or may not point to a handler.
///
///Hyperlinks has to be activated by setting `find_hyperlinks` to  `true`.

#[derive(Clone)]
pub struct TreeRouter<T> {
    items: HashMap<Method, (T, Vec<MaybeUtf8Owned>)>,
    static_routes: HashMap<MaybeUtf8Owned, TreeRouter<T>>,
    variable_route: Option<Box<TreeRouter<T>>>,
    wildcard_route: Option<Box<TreeRouter<T>>>,
    ///Should the router search for hyperlinks? Setting this to `true` may
    ///slow down endpoint search, but enables hyperlinks.
    pub find_hyperlinks: bool
}

impl<T> TreeRouter<T> {
    ///Creates an empty `TreeRouter`.
    pub fn new() -> TreeRouter<T> {
        TreeRouter::default()
    }

    //Tries to find a router matching the key or inserts a new one if none exists.
    fn find_or_insert_router<'a>(&'a mut self, key: &[u8]) -> &'a mut TreeRouter<T> {
        if let Some(&b'*') = key.get(0) {
            if self.wildcard_route.is_none() {
                self.wildcard_route = Some(Box::new(TreeRouter::new()));
            }
            &mut **self.wildcard_route.as_mut::<'a>().unwrap()
        } else if let Some(&b':') = key.get(0) {
            if self.variable_route.is_none() {
                self.variable_route = Some(Box::new(TreeRouter::new()));
            }
            &mut **self.variable_route.as_mut::<'a>().unwrap()
        } else {
            match self.static_routes.entry(key.to_owned().into()) {
                Occupied(entry) => entry.into_mut(),
                Vacant(entry) => entry.insert(TreeRouter::new())
            }
        }
    }

    ///Insert an other TreeRouter at a path. The content of the other TreeRouter will be merged with this one and
    ///content with the same path and method will be overwritten.
    pub fn insert_router<'r, R: Route<'r> + ?Sized>(&mut self, route: &'r R, router: TreeRouter<T>) {
        let (endpoint, variable_names) = route.segments().fold((self, Vec::new()),

            |(current, mut variable_names), piece| {
                let next = current.find_or_insert_router(&piece);
                match piece.get(0) {
                    Some(&b'*') | Some(&b':') => variable_names.push(piece[1..].to_owned().into()),
                    _ => {}
                }

                (next, variable_names)
            }

        );

        endpoint.merge_router(variable_names, router);
    }

    //Mergers this TreeRouter with an other TreeRouter.
    fn merge_router(&mut self, variable_names: Vec<MaybeUtf8Owned>, router: TreeRouter<T>) {
        for (key, (item, var_names)) in router.items {
            let mut new_var_names = variable_names.clone();
            new_var_names.extend(var_names);
            self.items.insert(key, (item, new_var_names));
        }

        for (key, router) in router.static_routes {
            let next = match self.static_routes.entry(key.clone()) {
                Occupied(entry) => entry.into_mut(),
                Vacant(entry) => entry.insert(TreeRouter::new())
            };
            next.merge_router(variable_names.clone(), router);
        }

        if let Some(router) = router.variable_route {
            if self.variable_route.is_none() {
                self.variable_route = Some(Box::new(TreeRouter::new()));
            }

            if let Some(ref mut next) = self.variable_route {
                next.merge_router(variable_names.clone(), *router);
            }
        }

        if let Some(router) = router.wildcard_route {
            if self.wildcard_route.is_none() {
                self.wildcard_route = Some(Box::new(TreeRouter::new()));
            }

            if let Some(ref mut next) = self.wildcard_route {
                next.merge_router(variable_names.clone(), *router);
            }
        }
    }
}

impl<T: Handler> Router for TreeRouter<T> {
    type Handler = T;

    fn find<'a>(&'a self, method: &Method, route: &[u8]) -> Endpoint<'a, T> {
        let path = route.segments().collect::<Vec<_>>();
        let mut variables = vec![None; path.len()];
        let mut stack = vec![(self, Wildcard, 0, 0), (self, Variable, 0, 0), (self, Static, 0, 0)];
        
        let mut result: Endpoint<T> = None.into();

        while let Some((current, branch, index, var_index)) = stack.pop() {
            if index == path.len() && result.handler.is_none() {
                if let Some(&(ref item, ref variable_names)) = current.items.get(&method) {
                    let values = path.iter().zip(variables.iter()).filter_map(|(v, keep)| {
                        if let &Some(index) = keep {
                            Some((index, v.clone()))
                        } else {
                            None
                        }
                    });

                    let mut var_map = HashMap::<MaybeUtf8Owned, MaybeUtf8Owned>::with_capacity(variable_names.len());
                    for (name, value) in VariableIter::new(values, variable_names) {
                        var_map.insert(name, value);
                    }

                    result.handler = Some(item);
                    result.variables = var_map;
                    if !self.find_hyperlinks {
                        return result;
                    }
                } else if !self.find_hyperlinks {
                    continue;
                }

                //Only register hyperlinks on the first pass.
                if branch == Static {
                    for (other_method, _) in &current.items {
                        if other_method != method {
                            result.hypermedia.links.push(Link {
                                method: Some(other_method.clone()),
                                path: vec![]
                            });
                        }
                    }

                    for (segment, _next) in &current.static_routes {
                        result.hypermedia.links.push(Link {
                            method: None,
                            path: vec![LinkSegment {
                                label: segment.as_slice(),
                                ty: SegmentType::Static
                            }]
                        });
                    }

                    if let Some(ref _next) = current.variable_route {
                        result.hypermedia.links.push(Link {
                            method: None,
                            path: vec![LinkSegment {
                                label: "".into(),
                                ty: SegmentType::VariableSegment
                            }]
                        });
                    }

                    if let Some(ref _next) = current.wildcard_route {
                        result.hypermedia.links.push(Link {
                            method: None,
                            path: vec![LinkSegment {
                                label: "".into(),
                                ty: SegmentType::VariableSequence
                            }]
                        });
                    }
                }
            }

            match branch {
                Static => {
                    if index < path.len() {
                        current.static_routes.get(path[index]).map(|next| {
                            variables.get_mut(index).map(|v| *v = None);

                            stack.push((next, Wildcard, index+1, var_index));
                            stack.push((next, Variable, index+1, var_index));
                            stack.push((next, Static, index+1, var_index));
                        });
                    }
                },
                Variable => {
                    if index < path.len() {
                        current.variable_route.as_ref().map(|next| {
                            variables.get_mut(index).map(|v| *v = Some(var_index));

                            stack.push((next, Wildcard, index+1, var_index+1));
                            stack.push((next, Variable, index+1, var_index+1));
                            stack.push((next, Static, index+1, var_index+1));
                        });
                    }
                },
                Wildcard => {
                    if index < path.len() {
                        current.wildcard_route.as_ref().map(|next| {
                            variables.get_mut(index).map(|v| *v = Some(var_index));

                            stack.push((current, Wildcard, index+1, var_index));
                            stack.push((next, Wildcard, index+1, var_index+1));
                            stack.push((next, Variable, index+1, var_index+1));
                            stack.push((next, Static, index+1, var_index+1));
                        });
                    }
                }
            }
        }

        result
    }

    fn insert<'a, D: ?Sized + Deref<Target=R> + 'a, R: ?Sized + Route<'a> + 'a>(&mut self, method: Method, route: &'a D, item: T) {
        let (endpoint, variable_names) = route.segments().fold((self, Vec::new()),
            |(current, mut variable_names), piece| {
                let next = current.find_or_insert_router(&piece);
                match piece.get(0) {
                    Some(&b'*') | Some(&b':') => variable_names.push(piece[1..].to_owned().into()),
                    _ => {}
                }

                (next, variable_names)
            }

        );

        endpoint.items.insert(method, (item, variable_names));
    }
}

impl<T: Handler, D: Deref<Target=R>, R: ?Sized + for<'a> Route<'a>> FromIterator<(Method, D, T)> for TreeRouter<T> {
    ///Create a `TreeRouter` from a collection of routes.
    ///
    ///```
    ///extern crate rustful;
    ///use rustful::Method::Get;
    ///use rustful::TreeRouter;
    ///# use rustful::{Handler, Context, Response};
    ///
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
    fn from_iter<I: IntoIterator<Item=(Method, D, T)>>(iterator: I) -> TreeRouter<T> {
        let mut root = TreeRouter::new();

        for (method, route, item) in iterator {
            root.insert(method, &route, item);
        }

        root
    }
}

impl<T> Default for TreeRouter<T> {
    fn default() -> TreeRouter<T> {
        TreeRouter {
            items: HashMap::new(),
            static_routes: HashMap::new(),
            variable_route: None,
            wildcard_route: None,
            find_hyperlinks: false
        }
    }
}


#[cfg(test)]
mod test {
    use super::TreeRouter;
    use router::Router;
    #[cfg(feature = "benchmark")]
    use test::Bencher;
    use context::Context;
    use context::hypermedia::{LinkSegment, SegmentType};
    use response::Response;
    use handler::Handler;
    use hyper::method::Method::{Get, Post, Delete, Put, Head};
    use Method;

    macro_rules! check {
        ($res: expr => $handler: expr, {$($key: expr => $val: expr),*}, [$([$($links: tt)*]),*]) => (
            {
                use std::collections::HashMap;
                use context::MaybeUtf8Slice;

                let handler: Option<&str> = $handler;
                let handler = handler.map(|h| TestHandler::from(h));
                let mut endpoint = $res;
                assert_eq!(endpoint.handler, handler.as_ref());
                let expected_vars: HashMap<MaybeUtf8Slice, MaybeUtf8Slice> = map!($($key => $val),*);
                for (key, expected) in &expected_vars {
                    if let Some(var) = endpoint.variables.get(key.as_ref()) {
                        if var != expected {
                            panic!("expected variable '{}' to have value '{}', but it is '{}'", key.as_utf8_lossy(), expected.as_utf8_lossy(), var.as_utf8_lossy());
                        }
                    } else {
                        panic!("variable '{}' is missing", key.as_utf8_lossy());
                    }
                }
                for (key, var) in &endpoint.variables {
                    if let Some(expected) = expected_vars.get(key.as_ref()) {
                        if var != expected {
                            panic!("found variable '{}' with value '{}', but it was expected to be '{}'", key.as_utf8_lossy(), var.as_utf8_lossy(), expected.as_utf8_lossy());
                        }
                    } else {
                        panic!("unexpected variable '{}'", key.as_utf8_lossy());
                    }
                }

                let expected_links = [$(link!($($links)*)),*];
                for link in &expected_links {
                    let index = endpoint
                                .hypermedia
                                .links
                                .iter()
                                .map(|link| (link.method.as_ref(), &link.path))
                                .position(|other| match (other, link) {
                                   ((Some(other_method), _), &SelfLink(ref method)) => other_method == method,
                                   ((None, other_link), &ForwardLink(ref link)) => other_link == link,
                                   _ => false
                                });
                    if let Some(index) = index {
                        endpoint.hypermedia.links.swap_remove(index);
                    } else {
                        panic!("missing link: {:?}", link);
                    }
                }
                assert_eq!(endpoint.hypermedia.links, vec![]);
            }
        );
        
        ($res: expr => $handler: expr, {$($key: expr => $val: expr),*}) => (
            check!($res => $handler, {$($key => $val),*}, []);
        );

        ($res: expr => $handler: expr, [$([$($links: tt)*]),*]) => (
            check!($res => $handler, {}, [$([$($links)*]),*]);
        );

        ($res: expr => $handler: expr) => (
            check!($res => $handler, {}, []);
        );
    }

    macro_rules! link {
        (./$method: ident) => (
            SelfLink($method)
        );
        ($($section: tt)+) => (
            {
                let mut link = vec![];
                add_link!(link $($section)+,);
                ForwardLink(link)
            }
        );
    }

    macro_rules! add_link {
        ($link: ident) => ();
        ($link: ident : $name: expr, $($rest: tt)*) => (
            {
                $link.push(LinkSegment {
                    label: $name.into(),
                    ty: SegmentType::VariableSegment
                });
                add_link!($link $($rest)*);
            }
        );
        ($link: ident * $name: expr, $($rest: tt)*) => (
            {
                $link.push(LinkSegment {
                    label: $name.into(),
                    ty: SegmentType::VariableSequence
                });
                add_link!($link $($rest)*);
            }
        );
        ($link: ident $name: expr, $($rest: tt)*) => (
            {
                $link.push(LinkSegment {
                    label: $name.into(),
                    ty: SegmentType::Static
                });
                add_link!($link $($rest)*);
            }
        );
    }

    macro_rules! map {
        () => (HashMap::new());
        ($($key: expr => $val: expr),*) => (
            {
                let mut map = HashMap::new();
                $(map.insert($key.into(), $val.into());)*
                map
            }
        );
    }

    #[derive(PartialEq, Debug, Clone, Copy)]
    struct TestHandler(&'static str);

    impl From<&'static str> for TestHandler {
        fn from(s: &'static str) -> TestHandler {
            TestHandler(s)
        }
    }

    impl Handler for TestHandler {
        fn handle_request(&self, _: Context, _: Response) {}
    }

    #[derive(Debug)]
    enum LinkType<'a> {
        SelfLink(Method),
        ForwardLink(Vec<LinkSegment<'a>>)
    }
    pub use self::LinkType::{SelfLink, ForwardLink};

    #[test]
    fn one_static_route() {
        let routes = vec![(Get, "path/to/test1", "test 1".into())];

        let mut router = routes.into_iter().collect::<TreeRouter<_>>();
        router.find_hyperlinks = true;

        check!(router.find(&Get, b"path/to/test1") => Some("test 1"));
        check!(router.find(&Get, b"path/to") => None, [["test1"]]);
        check!(router.find(&Get, b"path/to/test1/nothing") => None);
    }

    #[test]
    fn several_static_routes() {
        let routes = vec![
            (Get, "", "test 1".into()),
            (Get, "path/to/test/no2", "test 2".into()),
            (Get, "path/to/test1/no/test3", "test 3".into())
        ];

        let mut router = routes.into_iter().collect::<TreeRouter<_>>();
        router.find_hyperlinks = true;

        check!(router.find(&Get, b"") => Some("test 1"), [["path"]]);
        check!(router.find(&Get, b"path/to/test/no2") => Some("test 2"));
        check!(router.find(&Get, b"path/to/test1/no/test3") => Some("test 3"));
        check!(router.find(&Get, b"path/to/test1/no") => None, [["test3"]]);
    }

    #[test]
    fn one_variable_route() {
        let routes = vec![(Get, "path/:a/test1", "test_var".into())];

        let mut router = routes.into_iter().collect::<TreeRouter<_>>();
        router.find_hyperlinks = true;

        check!(router.find(&Get, b"path/to/test1") => Some("test_var"), {"a" => "to"});
        check!(router.find(&Get, b"path/to") => None, [["test1"]]);
        check!(router.find(&Get, b"path/to/test1/nothing") => None);
    }

    #[test]
    fn several_variable_routes() {
        let routes = vec![
            (Get, "path/to/test1", "test_var".into()),
            (Get, "path/:a/test/no2", "test_var".into()),
            (Get, "path/to/:b/:c/:a", "test_var".into()),
            (Post, "path/to/:c/:a/:b", "test_var".into())
        ];

        let mut router = routes.into_iter().collect::<TreeRouter<_>>();
        router.find_hyperlinks = true;

        check!(router.find(&Get, b"path/to/test1") => Some("test_var"));
        check!(router.find(&Get, b"path/to/test/no2") => Some("test_var"), {"a" => "to"}, [[:""]]);
        check!(router.find(&Get, b"path/to/test1/no/test3") => Some("test_var"), {"a" => "test3", "b" => "test1", "c" => "no"}, [[./Post]]);
        check!(router.find(&Post, b"path/to/test1/no/test3") => Some("test_var"), {"a" => "no", "b" => "test3", "c" => "test1"}, [[./Get]]);
        check!(router.find(&Get, b"path/to/test1/no") => None, [[:""]]);
    }

    #[test]
    fn one_wildcard_end_route() {
        let routes = vec![(Get, "path/to/*tail", "test 1".into())];

        let mut router = routes.into_iter().collect::<TreeRouter<_>>();
        router.find_hyperlinks = true;

        check!(router.find(&Get, b"path/to/test1") => Some("test 1"), {"tail" => "test1"});
        check!(router.find(&Get, b"path/to/same/test1") => Some("test 1"), {"tail" => "same/test1"});
        check!(router.find(&Get, b"path/to/the/same/test1") => Some("test 1"), {"tail" => "the/same/test1"});
        check!(router.find(&Get, b"path/to") => None, [[*""]]);
        check!(router.find(&Get, b"path") => None, [["to"]]);
    }


    #[test]
    fn one_wildcard_middle_route() {
        let routes = vec![(Get, "path/*middle/test1", "test 1".into())];

        let mut router = routes.into_iter().collect::<TreeRouter<_>>();
        router.find_hyperlinks = true;

        check!(router.find(&Get, b"path/to/test1") => Some("test 1"), {"middle" => "to"});
        check!(router.find(&Get, b"path/to/same/test1") => Some("test 1"), {"middle" => "to/same"});
        check!(router.find(&Get, b"path/to/the/same/test1") => Some("test 1"), {"middle" => "to/the/same"});
        check!(router.find(&Get, b"path/to") => None, [["test1"]]);
        check!(router.find(&Get, b"path") => None, [[*""]]);
   }

    #[test]
    fn one_universal_wildcard_route() {
        let routes = vec![(Get, "*all", "test 1".into())];

        let mut router = routes.into_iter().collect::<TreeRouter<_>>();
        router.find_hyperlinks = true;

        check!(router.find(&Get, b"path/to/test1") => Some("test 1"), {"all" => "path/to/test1"});
        check!(router.find(&Get, b"path/to/same/test1") => Some("test 1"), {"all" => "path/to/same/test1"});
        check!(router.find(&Get, b"path/to/the/same/test1") => Some("test 1"), {"all" => "path/to/the/same/test1"});
        check!(router.find(&Get, b"path/to") => Some("test 1"), {"all" => "path/to"});
        check!(router.find(&Get, b"path") => Some("test 1"), {"all" => "path"});
        check!(router.find(&Get, b"") => None, [[*""]]);
    }

    #[test]
    fn several_wildcards_routes() {
        let routes = vec![
            (Get, "path/to/*", "test 1".into()),
            (Get, "path/*/test/no2", "test 2".into()),
            (Get, "path/to/*/*/*", "test 3".into())
        ];

        let mut router = routes.into_iter().collect::<TreeRouter<_>>();
        router.find_hyperlinks = true;

        check!(router.find(&Get, b"path/to/test1") => Some("test 1"), [[*""]]);
        check!(router.find(&Get, b"path/for/test/no2") => Some("test 2"));
        check!(router.find(&Get, b"path/to/test1/no/test3") => Some("test 3"));
        check!(router.find(&Get, b"path/to/test1/no/test3/again") => Some("test 3"));
        check!(router.find(&Get, b"path/to") => None, [[*""], ["test"]]);
    }

    #[test]
    fn several_wildcards_routes_no_hyperlinks() {
        let routes = vec![
            (Get, "path/to/*tail", "test 1".into()),
            (Get, "path/*middle/test/no2", "test 2".into()),
            (Get, "path/to/*a/*b/*c", "test 3".into())
        ];

        let router = routes.into_iter().collect::<TreeRouter<_>>();

        check!(router.find(&Get, b"path/to/test1") => Some("test 1"), {"tail" => "test1"});
        check!(router.find(&Get, b"path/for/test/no2") => Some("test 2"), {"middle" => "for"});
        check!(router.find(&Get, b"path/to/test1/no/test3") => Some("test 3"), {"a" => "test1", "b" => "no", "c" => "test3"});
        check!(router.find(&Get, b"path/to/test1/no/test3/again") => Some("test 3"), {"a" => "test1", "b" => "no", "c" => "test3/again"});
        check!(router.find(&Get, b"path/to") => None);
    }

    #[test]
    fn route_formats() {
        let routes = vec![
            (Get, "/", "test 1".into()),
            (Get, "/path/to/test/no2", "test 2".into()),
            (Get, "path/to/test3/", "test 3".into()),
            (Get, "/path/to/test3/again/", "test 3".into())
        ];

        let mut router = routes.into_iter().collect::<TreeRouter<_>>();
        router.find_hyperlinks = true;

        check!(router.find(&Get, b"") => Some("test 1"), [["path"]]);
        check!(router.find(&Get, b"path/to/test/no2/") => Some("test 2"));
        check!(router.find(&Get, b"path/to/test3") => Some("test 3"), [["again"]]);
        check!(router.find(&Get, b"/path/to/test3/again") => Some("test 3"));
        check!(router.find(&Get, b"//path/to/test3") => None);
    }

    #[test]
    fn http_methods() {
        let routes = vec![
            (Get, "/", "get".into()),
            (Post, "/", "post".into()),
            (Delete, "/", "delete".into()),
            (Put, "/", "put".into())
        ];

        let mut router = routes.into_iter().collect::<TreeRouter<_>>();
        router.find_hyperlinks = true;
        check!(router.find(&Get, b"/") => Some("get"), [[./Post], [./Delete], [./Put]]);
        check!(router.find(&Post, b"/") => Some("post"), [[./Get], [./Delete], [./Put]]);
        check!(router.find(&Delete, b"/") => Some("delete"), [[./Get], [./Post], [./Put]]);
        check!(router.find(&Put, b"/") => Some("put"), [[./Get], [./Post], [./Delete]]);
        check!(router.find(&Head, b"/") => None, [[./Get], [./Post], [./Delete], [./Put]]);
    }

    #[test]
    fn merge_routers() {
        let routes1 = vec![
            (Get, "", "test 1".into()),
            (Get, "path/to/test/no2", "test 2".into()),
            (Get, "path/to/test1/no/test3", "test 3".into())
        ];

        let routes2 = vec![
            (Get, "", "test 1".into()),
            (Get, "test/no5", "test 5".into()),
            (Post, "test1/no/test3", "test 3 post".into())
        ];

        let mut router1 = routes1.into_iter().collect::<TreeRouter<_>>();
        router1.find_hyperlinks = true;
        let router2 = routes2.into_iter().collect::<TreeRouter<_>>();

        router1.insert_router("path/to", router2);

        check!(router1.find(&Get, b"") => Some("test 1"), [["path"]]);
        check!(router1.find(&Get, b"path/to/test/no2") => Some("test 2"));
        check!(router1.find(&Get, b"path/to/test1/no/test3") => Some("test 3"), [[./Post]]);
        check!(router1.find(&Post, b"path/to/test1/no/test3") => Some("test 3 post"), [[./Get]]);
        check!(router1.find(&Get, b"path/to/test/no5") => Some("test 5"));
        check!(router1.find(&Get, b"path/to") => Some("test 1"), [["test"], ["test1"]]);
    }


    #[test]
    fn merge_routers_variables() {
        let routes1 = vec![(Get, ":a/:b/:c", "test 2".into())];
        let routes2 = vec![(Get, ":b/:c/test", "test 1".into())];

        let mut router1 = routes1.into_iter().collect::<TreeRouter<_>>();
        router1.find_hyperlinks = true;
        let router2 = routes2.into_iter().collect::<TreeRouter<_>>();

        router1.insert_router(":a", router2);
        
        check!(router1.find(&Get, b"path/to/test1") => Some("test 2"), {"a" => "path", "b" => "to", "c" => "test1"}, [["test"]]);
        check!(router1.find(&Get, b"path/to/test1/test") => Some("test 1"), {"a" => "path", "b" => "to", "c" => "test1"});
    }


    #[test]
    fn merge_routers_wildcard() {
        let routes1 = vec![(Get, "path/to", "test 2".into())];
        let routes2 = vec![(Get, "*/test1", "test 1".into())];

        let mut router1 = routes1.into_iter().collect::<TreeRouter<_>>();
        router1.find_hyperlinks = true;
        let router2 = routes2.into_iter().collect::<TreeRouter<_>>();

        router1.insert_router("path", router2);

        check!(router1.find(&Get, b"path/to/test1") => Some("test 1"));
        check!(router1.find(&Get, b"path/to/same/test1") => Some("test 1"));
        check!(router1.find(&Get, b"path/to/the/same/test1") => Some("test 1"));
        check!(router1.find(&Get, b"path/to") => Some("test 2"));
        check!(router1.find(&Get, b"path") => None, [["to"], [*""]]);
    }

    
    #[bench]
    #[cfg(feature = "benchmark")]
    fn search_speed(b: &mut Bencher) {
        let routes = vec![
            (Get, "path/to/test1", "test 1".into()),
            (Get, "path/to/test/no2", "test 1".into()),
            (Get, "path/to/test1/no/test3", "test 1".into()),
            (Get, "path/to/other/test1", "test 1".into()),
            (Get, "path/to/test/no2/again", "test 1".into()),
            (Get, "other/path/to/test1/no/test3", "test 1".into()),
            (Get, "path/to/test1", "test 1".into()),
            (Get, "path/:a/test/no2", "test 1".into()),
            (Get, "path/to/:b/:c/:a", "test 1".into()),
            (Get, "path/to/*a", "test 1".into()),
            (Get, "path/to/*a/other", "test 1".into())
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

        let router = routes.into_iter().collect::<TreeRouter<TestHandler>>();
        let mut counter = 0;

        b.iter(|| {
            router.find(&Get, bpaths[counter]);
            counter = (counter + 1) % paths.len()
        });
    }

    
    #[bench]
    #[cfg(feature = "benchmark")]
    fn wildcard_speed(b: &mut Bencher) {
        let routes = vec![
            (Get, "*/to/*/*/a", "test 1".into())
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

        let router = routes.into_iter().collect::<TreeRouter<TestHandler>>();
        let mut counter = 0;

        b.iter(|| {
            router.find(&Get, bpaths[counter]);
            counter = (counter + 1) % paths.len()
        });
    }
}