use std::collections::HashMap;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::iter::{Iterator, IntoIterator, FromIterator};
use std::ops::Deref;
use hyper::method::Method;

use router::{Router, Route, Endpoint, MethodRouter, InsertState, RouteState, Variables};
use context::{MaybeUtf8Owned, MaybeUtf8Slice};
use context::hypermedia::{Link, LinkSegment, SegmentType};
use handler::Meta;

use self::Branch::{Static, Variable, Wildcard};

#[derive(PartialEq)]
enum Branch {
    Static,
    Variable,
    Wildcard
}

///A tree shaped router that selects handlers using paths.
///
///Each tree node stores an other router of type `T`, which has to implement
///`Default` to allow "empty" nodes, and a number of child references. The
///requested path must always be exhausted before sub-routers are searched, so
///there is no point in storing other path based routers in a `TreeRouter`.
///
///The `TreeRouter` has support for shallow hyperlinks to children, siblings,
///cousins, and so forth. The use of variable sequences complicates this
///process and may cause confusing results in certain situations. The
///hyperlinks may or may not point to a handler.
///
///Hyperlinks has to be activated by setting `find_hyperlinks` to  `true`.

#[derive(Clone)]
pub struct TreeRouter<T: Router + Default> {
    item: T,
    static_routes: HashMap<MaybeUtf8Owned, TreeRouter<T>>,
    variable_route: Option<Box<TreeRouter<T>>>,
    wildcard_route: Option<Box<TreeRouter<T>>>,
    ///Should the router search for hyperlinks? Setting this to `true` may
    ///slow down endpoint search, but enables hyperlinks.
    pub find_hyperlinks: bool
}

impl<H: Meta + Send + Sync> TreeRouter<MethodRouter<Variables<H>>> {
    ///Creates an empty `TreeRouter<MethodRouter<Variables<H>>>`, which is
    ///probably the most common composition. It will select handlers based on
    ///path and then HTTP method, and collect any variables on the way.
    ///
    ///Use `TreeRouter::default()` if this is not the desired composition.
    pub fn new() -> TreeRouter<MethodRouter<Variables<H>>> {
        TreeRouter::default()
    }
}


impl<T: Router + Default> TreeRouter<T> {
    //Tries to find a router matching the key or inserts a new one if none exists.
    fn find_or_insert_router<'a>(&'a mut self, key: &[u8]) -> &'a mut TreeRouter<T> {
        if let Some(&b'*') = key.get(0) {
            if self.wildcard_route.is_none() {
                self.wildcard_route = Some(Box::new(TreeRouter::default()));
            }
            &mut **self.wildcard_route.as_mut::<'a>().unwrap()
        } else if let Some(&b':') = key.get(0) {
            if self.variable_route.is_none() {
                self.variable_route = Some(Box::new(TreeRouter::default()));
            }
            &mut **self.variable_route.as_mut::<'a>().unwrap()
        } else {
            match self.static_routes.entry(key.to_owned().into()) {
                Occupied(entry) => entry.into_mut(),
                Vacant(entry) => entry.insert(TreeRouter::default())
            }
        }
    }

    //Mergers this TreeRouter with an other TreeRouter.
    fn merge_router<'a, I: Iterator<Item = &'a [u8]> + Clone>(&mut self, state: InsertState<'a, I>, router: TreeRouter<T>) {
        self.item.insert_router(state.clone(), router.item);

        for (key, router) in router.static_routes {
            let next = match self.static_routes.entry(key.clone()) {
                Occupied(entry) => entry.into_mut(),
                Vacant(entry) => entry.insert(TreeRouter::default())
            };
            next.merge_router(state.clone(), router);
        }

        if let Some(router) = router.variable_route {
            if self.variable_route.is_none() {
                self.variable_route = Some(Box::new(TreeRouter::default()));
            }

            if let Some(ref mut next) = self.variable_route {
                next.merge_router(state.clone(), *router);
            }
        }

        if let Some(router) = router.wildcard_route {
            if self.wildcard_route.is_none() {
                self.wildcard_route = Some(Box::new(TreeRouter::default()));
            }

            if let Some(ref mut next) = self.wildcard_route {
                next.merge_router(state, *router);
            }
        }
    }
}

impl<T: Router + Default> Router for TreeRouter<T> {
    type Handler = T::Handler;

    fn find<'a>(&'a self, method: &Method, route: &mut RouteState) -> Endpoint<'a, Self::Handler> {
        let now = route.snapshot();
        let mut stack = vec![(self, Wildcard, now), (self, Variable, now), (self, Static, now)];

        let mut result: Endpoint<Self::Handler> = None.into();

        while let Some((current, branch, snapshot)) = stack.pop() {
            route.go_to(snapshot);
            if route.is_empty() && result.handler.is_none() {
                let endpoint = current.item.find(&method, route);
                if endpoint.handler.is_some() {
                    result.handler = endpoint.handler;
                    result.variables = endpoint.variables;
                    if !self.find_hyperlinks {
                        return result;
                    }
                } else if !self.find_hyperlinks {
                    continue;
                }

                //Only register hyperlinks on the first pass.
                if branch == Static {
                    let base_link = Link {
                        method: None,
                        path: vec![],
                        handler: None
                    };

                    for link in current.item.hyperlinks(base_link) {
                        if link.method != Some(method.clone()) || !link.path.is_empty() {
                            result.hyperlinks.push(link);
                        }
                    }

                    for segment in current.static_routes.keys() {
                        result.hyperlinks.push(Link {
                            method: None,
                            path: vec![LinkSegment {
                                label: segment.as_slice(),
                                ty: SegmentType::Static
                            }],
                            handler: None
                        });
                    }

                    if let Some(ref _next) = current.variable_route {
                        result.hyperlinks.push(Link {
                            method: None,
                            path: vec![LinkSegment {
                                label: MaybeUtf8Slice::new(),
                                ty: SegmentType::VariableSegment
                            }],
                            handler: None
                        });
                    }

                    if let Some(ref _next) = current.wildcard_route {
                        result.hyperlinks.push(Link {
                            method: None,
                            path: vec![LinkSegment {
                                label: MaybeUtf8Slice::new(),
                                ty: SegmentType::VariableSequence
                            }],
                            handler: None
                        });
                    }
                }
            } else if let Some(segment) = route.get() {
                match branch {
                    Static => {
                        current.static_routes.get(segment).map(|next| {
                            route.skip();
                            let snapshot = route.snapshot();
                            stack.push((next, Wildcard, snapshot));
                            stack.push((next, Variable, snapshot));
                            stack.push((next, Static, snapshot));
                        });
                    },
                    Variable => {
                        current.variable_route.as_ref().map(|next| {
                            route.keep();
                            let snapshot = route.snapshot();
                            stack.push((next, Wildcard, snapshot));
                            stack.push((next, Variable, snapshot));
                            stack.push((next, Static, snapshot));
                        });
                    },
                    Wildcard => {
                        current.wildcard_route.as_ref().map(|next| {
                            route.fuse();
                            let s = route.snapshot();
                            stack.push((current, Wildcard, s));
                            route.go_to(snapshot);

                            route.keep();
                            let snapshot = route.snapshot();
                            stack.push((next, Wildcard, snapshot));
                            stack.push((next, Variable, snapshot));
                            stack.push((next, Static, snapshot));
                        });
                    }
                }
            }
        }

        result
    }

    fn hyperlinks<'a>(&'a self, base: Link<'a>) -> Vec<Link<'a>> {
        let mut links = self.item.hyperlinks(base.clone());

        for segment in self.static_routes.keys() {
            let mut link = base.clone();
            link.path.push(LinkSegment {
                label: segment.as_slice(),
                ty: SegmentType::Static
            });
            links.push(link);
        }

        if let Some(ref _next) = self.variable_route {
            let mut link = base.clone();
            link.path.push(LinkSegment {
                label: MaybeUtf8Slice::new(),
                ty: SegmentType::VariableSegment
            });
            links.push(link);
        }

        if let Some(ref _next) = self.wildcard_route {
            let mut link = base;
            link.path.push(LinkSegment {
                label: MaybeUtf8Slice::new(),
                ty: SegmentType::VariableSequence
            });
            links.push(link);
        }

        links
    }

    fn build<'a, R: Into<InsertState<'a, I>>, I: Iterator<Item = &'a [u8]>>(method: Method, route: R, item: Self::Handler) -> TreeRouter<T> {
        let mut router = TreeRouter::default();
        router.insert(method, route, item);
        router
    }

    fn insert<'a, R: Into<InsertState<'a, I>>, I: Iterator<Item = &'a [u8]>>(&mut self, method: Method, route: R, item: Self::Handler) {
        let mut route = route.into();
        let mut endpoint = (&mut route).fold(self, |endpoint, segment| {
            endpoint.find_or_insert_router(segment)
        });

        endpoint.item.insert(method, route, item);
    }

    fn insert_router<'a, R: Into<InsertState<'a, I>>, I: Clone + Iterator<Item = &'a [u8]>>(&mut self, route: R, router: TreeRouter<T>) {
        let mut route = route.into();
        let mut endpoint = (&mut route).fold(self, |endpoint, segment| {
            endpoint.find_or_insert_router(segment)
        });

        endpoint.merge_router(route, router);
    }

    fn prefix<'a, R: Into<InsertState<'a, I>>, I: Clone + Iterator<Item = &'a [u8]>>(&mut self, route: R) {
        let mut route = route.into();
        if !route.is_empty() {
            let mut new_root = TreeRouter::default();
            new_root.find_hyperlinks = self.find_hyperlinks;
            {
                let mut endpoint = (&mut route).fold(&mut new_root, |endpoint, segment| {
                    endpoint.find_or_insert_router(segment)
                });

                ::std::mem::swap(endpoint, self);
            }
            *self = new_root;
        }

        for (_, router) in &mut self.static_routes {
            router.prefix(route.clone());
        }

        if let Some(router) = self.variable_route.as_mut() {
            router.prefix(route.clone());
        }

        if let Some(router) = self.wildcard_route.as_mut() {
            router.prefix(route.clone());
        }

        self.item.prefix(route);
    }
}

impl<T: Meta + Send + Sync, D: Deref<Target=R>, R: ?Sized + for<'a> Route<'a>> FromIterator<(Method, D, T)> for TreeRouter<MethodRouter<Variables<T>>> {
    ///Create a `TreeRouter` from a collection of routes.
    ///
    ///```
    ///extern crate rustful;
    ///use rustful::Method::Get;
    ///use rustful::TreeRouter;
    ///# use rustful::{Handler, Context, Response};
    ///
    ///# struct DummyHandler;
    ///# impl<'env> Handler<'env> for DummyHandler {
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
    fn from_iter<I: IntoIterator<Item=(Method, D, T)>>(iterator: I) -> TreeRouter<MethodRouter<Variables<T>>> {
        let mut root = TreeRouter::new();

        for (method, route, item) in iterator {
            root.insert(method, &*route, item);
        }

        root
    }
}

impl<T: Default + Router> Default for TreeRouter<T> {
    fn default() -> TreeRouter<T> {
        TreeRouter {
            item: T::default(),
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
        ($router: ident ($method: expr, $path: expr) => $handler: expr, {$($key: expr => $val: expr),*}, [$([$($links: tt)*]),*]) => (
            {
                use std::collections::HashMap;
                use context::MaybeUtf8Slice;

                let handler: Option<&str> = $handler;
                let handler = handler.map(TestHandler::from);
                let mut endpoint = $router.find($method, &mut ((&$path[..]).into()));
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
                                .hyperlinks
                                .iter()
                                .map(|link| (link.method.as_ref(), &link.path))
                                .position(|other| match (other, link) {
                                   ((Some(other_method), _), &SelfLink(ref method)) => other_method == method,
                                   ((None, other_link), &ForwardLink(ref link)) => other_link == link,
                                   _ => false
                                });
                    if let Some(index) = index {
                        endpoint.hyperlinks.swap_remove(index);
                    } else {
                        panic!("missing link: {:?}", link);
                    }
                }
                for link in endpoint.hyperlinks {
                    panic!("unexpected hyperlink: {:?}", link);
                }
            }
        );
        
        ($router: ident ($method: expr, $path: expr) => $handler: expr, {$($key: expr => $val: expr),*}) => (
            check!($router ($method, $path) => $handler, {$($key => $val),*}, []);
        );

        ($router: ident ($method: expr, $path: expr) => $handler: expr, [$([$($links: tt)*]),*]) => (
            check!($router ($method, $path) => $handler, {}, [$([$($links)*]),*]);
        );

        ($router: ident ($method: expr, $path: expr) => $handler: expr) => (
            check!($router ($method, $path) => $handler, {}, []);
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

    impl<'env> Handler<'env> for TestHandler {
        fn handle_request(&self, _: Context, _: Response) {}
    }

    #[derive(Debug)]
    enum LinkType<'a> {
        SelfLink(Method),
        ForwardLink(Vec<LinkSegment<'a>>)
    }
    use self::LinkType::{SelfLink, ForwardLink};

    #[test]
    fn one_static_route() {
        let routes = vec![(Get, "path/to/test1", "test 1".into())];

        let mut router = routes.into_iter().collect::<TreeRouter<_>>();
        router.find_hyperlinks = true;

        check!(router(&Get, b"path/to/test1") => Some("test 1"));
        check!(router(&Get, b"path/to") => None, [["test1"]]);
        check!(router(&Get, b"path/to/test1/nothing") => None);
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

        check!(router(&Get, b"") => Some("test 1"), [["path"]]);
        check!(router(&Get, b"path/to/test/no2") => Some("test 2"));
        check!(router(&Get, b"path/to/test1/no/test3") => Some("test 3"));
        check!(router(&Get, b"path/to/test1/no") => None, [["test3"]]);
    }

    #[test]
    fn one_variable_route() {
        let routes = vec![(Get, "path/:a/test1", "test_var".into())];

        let mut router = routes.into_iter().collect::<TreeRouter<_>>();
        router.find_hyperlinks = true;

        check!(router(&Get, b"path/to/test1") => Some("test_var"), {"a" => "to"});
        check!(router(&Get, b"path/to") => None, [["test1"]]);
        check!(router(&Get, b"path/to/test1/nothing") => None);
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

        check!(router(&Get, b"path/to/test1") => Some("test_var"));
        check!(router(&Get, b"path/to/test/no2") => Some("test_var"), {"a" => "to"}, [[:""]]);
        check!(router(&Get, b"path/to/test1/no/test3") => Some("test_var"), {"a" => "test3", "b" => "test1", "c" => "no"}, [[./Post]]);
        check!(router(&Post, b"path/to/test1/no/test3") => Some("test_var"), {"a" => "no", "b" => "test3", "c" => "test1"}, [[./Get]]);
        check!(router(&Get, b"path/to/test1/no") => None, [[:""]]);
    }

    #[test]
    fn one_wildcard_end_route() {
        let routes = vec![(Get, "path/to/*tail", "test 1".into())];

        let mut router = routes.into_iter().collect::<TreeRouter<_>>();
        router.find_hyperlinks = true;

        check!(router(&Get, b"path/to/test1") => Some("test 1"), {"tail" => "test1"});
        check!(router(&Get, b"path/to/same/test1") => Some("test 1"), {"tail" => "same/test1"});
        check!(router(&Get, b"path/to/the/same/test1") => Some("test 1"), {"tail" => "the/same/test1"});
        check!(router(&Get, b"path/to") => None, [[*""]]);
        check!(router(&Get, b"path") => None, [["to"]]);
    }


    #[test]
    fn one_wildcard_middle_route() {
        let routes = vec![(Get, "path/*middle/test1", "test 1".into())];

        let mut router = routes.into_iter().collect::<TreeRouter<_>>();
        router.find_hyperlinks = true;

        check!(router(&Get, b"path/to/test1") => Some("test 1"), {"middle" => "to"});
        check!(router(&Get, b"path/to/same/test1") => Some("test 1"), {"middle" => "to/same"});
        check!(router(&Get, b"path/to/the/same/test1") => Some("test 1"), {"middle" => "to/the/same"});
        check!(router(&Get, b"path/to") => None, [["test1"]]);
        check!(router(&Get, b"path") => None, [[*""]]);
   }

    #[test]
    fn one_universal_wildcard_route() {
        let routes = vec![(Get, "*all", "test 1".into())];

        let mut router = routes.into_iter().collect::<TreeRouter<_>>();
        router.find_hyperlinks = true;

        check!(router(&Get, b"path/to/test1") => Some("test 1"), {"all" => "path/to/test1"});
        check!(router(&Get, b"path/to/same/test1") => Some("test 1"), {"all" => "path/to/same/test1"});
        check!(router(&Get, b"path/to/the/same/test1") => Some("test 1"), {"all" => "path/to/the/same/test1"});
        check!(router(&Get, b"path/to") => Some("test 1"), {"all" => "path/to"});
        check!(router(&Get, b"path") => Some("test 1"), {"all" => "path"});
        check!(router(&Get, b"") => None, [[*""]]);
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

        check!(router(&Get, b"path/to/test1") => Some("test 1"), [[*""]]);
        check!(router(&Get, b"path/for/test/no2") => Some("test 2"));
        check!(router(&Get, b"path/to/test1/no/test3") => Some("test 3"));
        check!(router(&Get, b"path/to/test1/no/test3/again") => Some("test 3"));
        check!(router(&Get, b"path/to") => None, [[*""], ["test"]]);
    }

    #[test]
    fn several_wildcards_routes_no_hyperlinks() {
        let routes = vec![
            (Get, "path/to/*tail", "test 1".into()),
            (Get, "path/*middle/test/no2", "test 2".into()),
            (Get, "path/to/*a/*b/*c", "test 3".into())
        ];

        let router = routes.into_iter().collect::<TreeRouter<_>>();

        check!(router(&Get, b"path/to/test1") => Some("test 1"), {"tail" => "test1"});
        check!(router(&Get, b"path/for/test/no2") => Some("test 2"), {"middle" => "for"});
        check!(router(&Get, b"path/to/test1/no/test3") => Some("test 3"), {"a" => "test1", "b" => "no", "c" => "test3"});
        check!(router(&Get, b"path/to/test1/no/test3/again") => Some("test 3"), {"a" => "test1", "b" => "no", "c" => "test3/again"});
        check!(router(&Get, b"path/to") => None);
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

        check!(router(&Get, b"") => Some("test 1"), [["path"]]);
        check!(router(&Get, b"path/to/test/no2/") => Some("test 2"));
        check!(router(&Get, b"path/to/test3") => Some("test 3"), [["again"]]);
        check!(router(&Get, b"/path/to/test3/again") => Some("test 3"));
        check!(router(&Get, b"//path/to/test3") => None);
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
        check!(router(&Get, b"/") => Some("get"), [[./Post], [./Delete], [./Put]]);
        check!(router(&Post, b"/") => Some("post"), [[./Get], [./Delete], [./Put]]);
        check!(router(&Delete, b"/") => Some("delete"), [[./Get], [./Post], [./Put]]);
        check!(router(&Put, b"/") => Some("put"), [[./Get], [./Post], [./Delete]]);
        check!(router(&Head, b"/") => None, [[./Get], [./Post], [./Delete], [./Put]]);
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

        check!(router1(&Get, b"") => Some("test 1"), [["path"]]);
        check!(router1(&Get, b"path/to/test/no2") => Some("test 2"));
        check!(router1(&Get, b"path/to/test1/no/test3") => Some("test 3"), [[./Post]]);
        check!(router1(&Post, b"path/to/test1/no/test3") => Some("test 3 post"), [[./Get]]);
        check!(router1(&Get, b"path/to/test/no5") => Some("test 5"));
        check!(router1(&Get, b"path/to") => Some("test 1"), [["test"], ["test1"]]);
    }


    #[test]
    fn merge_routers_variables() {
        let routes1 = vec![(Get, ":a/:b/:c", "test 2".into())];
        let routes2 = vec![(Get, ":b/:c/test", "test 1".into())];

        let mut router1 = routes1.into_iter().collect::<TreeRouter<_>>();
        router1.find_hyperlinks = true;
        let router2 = routes2.into_iter().collect::<TreeRouter<_>>();

        router1.insert_router(":a", router2);
        
        check!(router1(&Get, b"path/to/test1") => Some("test 2"), {"a" => "path", "b" => "to", "c" => "test1"}, [["test"]]);
        check!(router1(&Get, b"path/to/test1/test") => Some("test 1"), {"a" => "path", "b" => "to", "c" => "test1"});
    }


    #[test]
    fn merge_routers_wildcard() {
        let routes1 = vec![(Get, "path/to", "test 2".into())];
        let routes2 = vec![(Get, "*/test1", "test 1".into())];

        let mut router1 = routes1.into_iter().collect::<TreeRouter<_>>();
        router1.find_hyperlinks = true;
        let router2 = routes2.into_iter().collect::<TreeRouter<_>>();

        router1.insert_router("path", router2);

        check!(router1(&Get, b"path/to/test1") => Some("test 1"));
        check!(router1(&Get, b"path/to/same/test1") => Some("test 1"));
        check!(router1(&Get, b"path/to/the/same/test1") => Some("test 1"));
        check!(router1(&Get, b"path/to") => Some("test 2"));
        check!(router1(&Get, b"path") => None, [["to"], [*""]]);
    }

    #[test]
    fn prefix_router_variables() {
        let routes1 = vec![
            (Get, ":b/:c", "test 2".into()),
            (Get, ":b/:c/test", "test 1".into())
        ];

        let mut router1 = routes1.into_iter().collect::<TreeRouter<_>>();
        router1.find_hyperlinks = true;
        router1.prefix(":a");
        
        check!(router1(&Get, b"path/to/test1") => Some("test 2"), {"a" => "path", "b" => "to", "c" => "test1"}, [["test"]]);
        check!(router1(&Get, b"path/to/test1/test") => Some("test 1"), {"a" => "path", "b" => "to", "c" => "test1"});
    }

    #[test]
    fn prefix_router_wildcard() {
        let routes1 = vec![
            (Get, "path/to", "test 2".into()),
            (Get, "*/test1", "test 1".into()),
        ];

        let mut router1 = routes1.into_iter().collect::<TreeRouter<_>>();
        router1.find_hyperlinks = true;
        router1.prefix(":a");

        check!(router1(&Get, b"a/path/to/test1") => Some("test 1"), {"a" => "a"});
        check!(router1(&Get, b"a/path/to/same/test1") => Some("test 1"), {"a" => "a"});
        check!(router1(&Get, b"a/path/to/the/same/test1") => Some("test 1"), {"a" => "a"});
        check!(router1(&Get, b"a/path/to") => Some("test 2"), {"a" => "a"});
        check!(router1(&Get, b"a/path") => None, [["to"], ["test1"]]);
    }
    
    #[bench]
    #[cfg(feature = "benchmark")]
    fn search_speed(b: &mut Bencher) {
        let routes: Vec<(_, _, TestHandler)> = vec![
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

        let router = routes.into_iter().collect::<TreeRouter<_>>();
        let mut counter = 0;

        b.iter(|| {
            router.find(&Get, &mut (paths[counter].into()));
            counter = (counter + 1) % paths.len()
        });
    }

    
    #[bench]
    #[cfg(feature = "benchmark")]
    fn wildcard_speed(b: &mut Bencher) {
        let routes: Vec<(_, _, TestHandler)> = vec![
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

        let router = routes.into_iter().collect::<TreeRouter<_>>();
        let mut counter = 0;

        b.iter(|| {
            router.find(&Get, &mut (paths[counter].into()));
            counter = (counter + 1) % paths.len()
        });
    }
}
