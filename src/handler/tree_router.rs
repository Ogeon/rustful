use std::collections::HashMap;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::iter::{Iterator, IntoIterator, FromIterator};
use std::ops::Deref;
use hyper::method::Method;

use context::{MaybeUtf8Owned, MaybeUtf8Slice};
use context::hypermedia::{Link, LinkSegment, SegmentType};
use handler::Handler;
use handler::{HandleRequest, Environment, MethodRouter, Variables};
use handler::routing::{Insert, InsertExt, Route, InsertState};
use StatusCode;

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
pub struct TreeRouter<T> {
    item: T,
    static_routes: HashMap<MaybeUtf8Owned, TreeRouter<T>>,
    variable_route: Option<Box<TreeRouter<T>>>,
    wildcard_route: Option<Box<TreeRouter<T>>>,
    ///Should the router search for hyperlinks? Setting this to `true` may
    ///slow down endpoint search, but enables hyperlinks.
    pub find_hyperlinks: bool
}

impl<T: Default> TreeRouter<T> {
    ///Creates an empty `TreeRouter`.
    pub fn new() -> TreeRouter<T> {
        TreeRouter::default()
    }

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
}

impl<T: InsertExt + Default> TreeRouter<T> {
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

impl<T: Insert<H> + Default, H> Insert<H> for TreeRouter<T> {
    fn build<'a, R: Into<InsertState<'a, I>>, I: Iterator<Item = &'a [u8]>>(method: Method, route: R, item: H) -> TreeRouter<T> {
        let mut router = TreeRouter::default();
        router.insert(method, route, item);
        router
    }

    fn insert<'a, R: Into<InsertState<'a, I>>, I: Iterator<Item = &'a [u8]>>(&mut self, method: Method, route: R, item: H) {
        let mut route = route.into();
        let mut endpoint = (&mut route).fold(self, |endpoint, segment| {
            endpoint.find_or_insert_router(segment)
        });

        endpoint.item.insert(method, route, item);
    }
}

impl<T: InsertExt + Default> InsertExt for TreeRouter<T> {
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

impl<T: HandleRequest> HandleRequest for TreeRouter<T> {
    fn handle_request<'a, 'b, 'l, 'g>(&self, mut environment: Environment<'a, 'b, 'l, 'g>) -> Result<(), Environment<'a, 'b, 'l, 'g>> {
        let now = environment.route_state.snapshot();
        let mut stack = vec![(self, Wildcard, now), (self, Variable, now), (self, Static, now)];

        let mut hyperlinks = vec![];
        let mut matches = vec![];

        while let Some((current, branch, snapshot)) = stack.pop() {
            environment.route_state.go_to(snapshot);
            if environment.route_state.is_empty() {
                if !self.find_hyperlinks {
                    let (new_environment, old_hyperlinks) = environment.replace_hyperlinks(vec![]);
                    if let Err(returned_environment) = current.item.handle_request(new_environment) {
                        environment = returned_environment.replace_hyperlinks(old_hyperlinks).0;
                    } else {
                        return Ok(());
                    };

                    continue;
                }

                matches.push((&current.item, environment.route_state.clone()));

                if branch == Static {
                    let base_link = Link {
                        method: None,
                        path: vec![],
                        handler: None
                    };

                    for link in current.item.hyperlinks(base_link) {
                        if link.method != Some(environment.context.method.clone()) || !link.path.is_empty() {
                            hyperlinks.push(link);
                        }
                    }

                    for segment in current.static_routes.keys() {
                        hyperlinks.push(Link {
                            method: None,
                            path: vec![LinkSegment {
                                label: segment.as_slice(),
                                ty: SegmentType::Static
                            }],
                            handler: None
                        });
                    }

                    if let Some(ref _next) = current.variable_route {
                        hyperlinks.push(Link {
                            method: None,
                            path: vec![LinkSegment {
                                label: MaybeUtf8Slice::new(),
                                ty: SegmentType::VariableSegment
                            }],
                            handler: None
                        });
                    }

                    if let Some(ref _next) = current.wildcard_route {
                        hyperlinks.push(Link {
                            method: None,
                            path: vec![LinkSegment {
                                label: MaybeUtf8Slice::new(),
                                ty: SegmentType::VariableSequence
                            }],
                            handler: None
                        });
                    }
                }
            } else if let Some(segment) = environment.route_state.get() {
                match branch {
                    Static => {
                        current.static_routes.get(segment).map(|next| {
                            environment.route_state.skip();
                            let snapshot = environment.route_state.snapshot();
                            stack.push((next, Wildcard, snapshot));
                            stack.push((next, Variable, snapshot));
                            stack.push((next, Static, snapshot));
                        });
                    },
                    Variable => {
                        current.variable_route.as_ref().map(|next| {
                            environment.route_state.keep();
                            let snapshot = environment.route_state.snapshot();
                            stack.push((next, Wildcard, snapshot));
                            stack.push((next, Variable, snapshot));
                            stack.push((next, Static, snapshot));
                        });
                    },
                    Wildcard => {
                        current.wildcard_route.as_ref().map(|next| {
                            environment.route_state.fuse();
                            let s = environment.route_state.snapshot();
                            stack.push((current, Wildcard, s));
                            environment.route_state.go_to(snapshot);

                            environment.route_state.keep();
                            let snapshot = environment.route_state.snapshot();
                            stack.push((next, Wildcard, snapshot));
                            stack.push((next, Variable, snapshot));
                            stack.push((next, Static, snapshot));
                        });
                    }
                }
            }
        }

        if self.find_hyperlinks {
            hyperlinks.sort();
            hyperlinks.dedup();
            let (mut new_environment, old_hyperlinks) = environment.replace_hyperlinks(hyperlinks);

            for (handler, snapshot) in matches {
                new_environment.route_state = snapshot;
                if let Err(returned_environment) = handler.handle_request(new_environment) {
                    new_environment = returned_environment;
                } else {
                    return Ok(());
                };
            }

            environment = new_environment.replace_hyperlinks(old_hyperlinks).0;
        }

        if environment.response.status().is_success() {
            environment.response.set_status(StatusCode::NotFound);
        }

        Err(environment)
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
}

impl<T: Handler + 'static, D: Deref<Target=R>, R: ?Sized + for<'a> Route<'a>> FromIterator<(Method, D, T)> for TreeRouter<MethodRouter<Variables<T>>> {
    ///Create a `TreeRouter` from a collection of routes.
    ///
    ///```
    ///extern crate rustful;
    ///use rustful::Method::Get;
    ///use rustful::handler::TreeRouter;
    ///# use rustful::{Handler, Context, Response};
    ///
    ///# struct DummyHandler;
    ///# impl Handler for DummyHandler {
    ///#     fn handle(&self, _: Context, _: Response){}
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

impl<T: Default> Default for TreeRouter<T> {
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
    use std::sync::{Arc, Mutex};
    use std::collections::HashMap;

    use super::TreeRouter;
    #[cfg(feature = "benchmark")]
    use test::Bencher;
    use context::Context;
    use context::{MaybeUtf8Slice, Parameters};
    use context::hypermedia::{LinkSegment, SegmentType};
    use response::Response;
    use handler::{Handler, MethodRouter, Variables};
    use handler::routing::{Insert, InsertExt};
    use hyper::method::Method::{Get, Post, Delete, Put, Head};
    use Method;

    type TestRouter = TreeRouter<MethodRouter<Variables<TestHandler>>>;

    macro_rules! route {
        ($router: ident ($method: expr, $path: expr), [$([$($links: tt)*]),*]) => (
            {
                let state = Arc::new(Mutex::new(HandlerState::new()));
                let handler = TestHandler {
                    state: state.clone(),
                    links: vec![$(link!($($links)*)),*],
                };

                $router.insert($method, $path, handler);
                state
            }
        );

        ($router: ident ($method: expr, $path: expr)) => (
            route!($router ($method, $path), []);
        );
    }

    macro_rules! check {
        ($router: ident ($method: expr, $path: expr) => $handler: expr, {$($key: expr => $val: expr),*}) => (
            {
                use handler::{Environment, HandleRequest};
                use header::Headers;
                use server::Global;

                let global = Global::default();
                let mut context = Context::mock($method, $path, Headers::new(), &global);
                context.method = $method;
                let response = Response::mock(&global);

                let result = $router.handle_request(Environment {
                    context: context,
                    response: response,
                    route_state: $path.into(),
                });

                let handler: Option<&Arc<Mutex<HandlerState>>> = $handler;

                if let Some(handler) = handler {
                    let mut handler_state = handler.lock().unwrap();
                    let variables: HashMap<MaybeUtf8Slice<'static>, MaybeUtf8Slice<'static>> = map!($($key => $val),*);

                    assert!(handler_state.visited);
                    assert!(result.is_ok());

                    for (key, expected) in &variables {
                        if let Some(var) = handler_state.variables.get(key.as_ref()) {
                            if var != expected.as_utf8_lossy() {
                                panic!("expected variable '{}' to have value '{}', but it is '{}'", key.as_utf8_lossy(), expected.as_utf8_lossy(), var);
                            }
                        } else {
                            panic!("variable '{}' is missing", key.as_utf8_lossy());
                        }
                    }
                    for (key, var) in &handler_state.variables {
                        if let Some(expected) = variables.get(key.as_ref()) {
                            if var != expected {
                                panic!("found variable '{}' with value '{}', but it was expected to be '{}'", key.as_utf8_lossy(), var.as_utf8_lossy(), expected.as_utf8_lossy());
                            }
                        } else {
                            panic!("unexpected variable '{}'", key.as_utf8_lossy());
                        }
                    }

                    handler_state.reset();
                } else {
                    assert!(result.is_err());
                }
            }
        );

        ($router: ident ($method: expr, $path: expr) => $handler: expr) => (
            check!($router($method, $path) => $handler, {})
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

    struct HandlerState {
        visited: bool,
        variables: Parameters,
    }

    impl HandlerState {
        fn new() -> HandlerState {
            HandlerState {
                visited: false,
                variables: Parameters::new(),
            }
        }

        fn reset(&mut self) {
            *self = HandlerState::new()
        }
    }

    struct TestHandler {
        state: Arc<Mutex<HandlerState>>,
        links: Vec<LinkType<'static>>,
    }

    impl Handler for TestHandler {
        fn handle(&self, mut context: Context, _: Response) {
            for link in &self.links {
                let index = context
                    .hyperlinks
                    .iter()
                    .map(|link| (link.method.as_ref(), &link.path))
                    .position(|other| match (other, link) {
                       ((Some(other_method), _), &SelfLink(ref method)) => other_method == method,
                       ((None, other_link), &ForwardLink(ref link)) => other_link == link,
                       _ => false
                    });
                if let Some(index) = index {
                    context.hyperlinks.swap_remove(index);
                } else {
                    panic!("missing link: {:?} (path: {})", link, context.uri_path);
                }
            }
            for link in context.hyperlinks {
                panic!("unexpected hyperlink: {:?} (path: {})", link, context.uri_path);
            }

            let mut state = self.state.lock().unwrap();
            state.variables = context.variables;
            state.visited = true;

        }
    }

    #[derive(Debug)]
    enum LinkType<'a> {
        SelfLink(Method),
        ForwardLink(Vec<LinkSegment<'a>>)
    }
    use self::LinkType::{SelfLink, ForwardLink};

    #[test]
    fn one_static_route() {
        let mut router = TestRouter::new();
        router.find_hyperlinks = true;

        let test1 = route!(router(Get, "path/to/test1"));

        check!(router(Get, "path/to/test1") => Some(&test1));
        check!(router(Get, "path/to") => None);
        check!(router(Get, "path/to/test1/nothing") => None);
    }

    #[test]
    fn several_static_routes() {
        let mut router = TestRouter::new();
        router.find_hyperlinks = true;

        let test1 = route!(router(Get, ""), [["path"]]);
        let test2 = route!(router(Get, "path/to/test/no2"));
        let test3 = route!(router(Get, "path/to/test1/no/test3"));

        check!(router(Get, "") => Some(&test1));
        check!(router(Get, "path/to/test/no2") => Some(&test2));
        check!(router(Get, "path/to/test1/no/test3") => Some(&test3));
        check!(router(Get, "path/to/test1/no") => None);
    }

    #[test]
    fn one_variable_route() {
        let mut router = TestRouter::new();
        router.find_hyperlinks = true;

        let test1 = route!(router(Get, "path/:a/test1"));

        check!(router(Get, "path/to/test1") => Some(&test1), {"a" => "to"});
        check!(router(Get, "path/to") => None);
        check!(router(Get, "path/to/test1/nothing") => None);
    }

    #[test]
    fn several_variable_routes() {
        let mut router = TestRouter::new();
        router.find_hyperlinks = true;

        let test1 = route!(router(Get, "path/to/test1"), [[:""]]);
        let test2 = route!(router(Get, "path/:a/test/no2"), [[:""]]);
        let test3_get = route!(router(Get, "path/to/:b/:c/:a"), [[./Post]]);
        let test3_post = route!(router(Post, "path/to/:c/:a/:b"), [[./Get]]);

        check!(router(Get, "path/to/test1") => Some(&test1));
        check!(router(Get, "path/to/test/no2") => Some(&test2), {"a" => "to"});
        check!(router(Get, "path/to/test1/no/test3") => Some(&test3_get), {"a" => "test3", "b" => "test1", "c" => "no"});
        check!(router(Post, "path/to/test1/no/test3") => Some(&test3_post), {"a" => "no", "b" => "test3", "c" => "test1"});
        check!(router(Get, "path/to/test1/no") => None);
    }

    #[test]
    fn one_wildcard_end_route() {
        let mut router = TestRouter::new();
        router.find_hyperlinks = true;

        let test1 = route!(router(Get, "path/to/*tail"));

        check!(router(Get, "path/to/test1") => Some(&test1), {"tail" => "test1"});
        check!(router(Get, "path/to/same/test1") => Some(&test1), {"tail" => "same/test1"});
        check!(router(Get, "path/to/the/same/test1") => Some(&test1), {"tail" => "the/same/test1"});
        check!(router(Get, "path/to") => None);
        check!(router(Get, "path") => None);
    }


    #[test]
    fn one_wildcard_middle_route() {
        let mut router = TestRouter::new();
        router.find_hyperlinks = true;

        let test1 = route!(router(Get, "path/*middle/test1"), [["test1"]]);

        check!(router(Get, "path/to/test1") => Some(&test1), {"middle" => "to"});
        check!(router(Get, "path/to/same/test1") => Some(&test1), {"middle" => "to/same"});
        check!(router(Get, "path/to/the/same/test1") => Some(&test1), {"middle" => "to/the/same"});
        check!(router(Get, "path/to") => None);
        check!(router(Get, "path") => None);
   }

    #[test]
    fn one_universal_wildcard_route() {
        let mut router = TestRouter::new();
        router.find_hyperlinks = true;

        let test1 = route!(router(Get, "*all"));

        check!(router(Get, "path/to/test1") => Some(&test1), {"all" => "path/to/test1"});
        check!(router(Get, "path/to/same/test1") => Some(&test1), {"all" => "path/to/same/test1"});
        check!(router(Get, "path/to/the/same/test1") => Some(&test1), {"all" => "path/to/the/same/test1"});
        check!(router(Get, "path/to") => Some(&test1), {"all" => "path/to"});
        check!(router(Get, "path") => Some(&test1), {"all" => "path"});
        check!(router(Get, "") => None);
    }

    #[test]
    fn several_wildcards_routes() {
        let mut router = TestRouter::new();
        router.find_hyperlinks = true;

        let test1 = route!(router(Get, "path/to/*"), [[*""], ["test"]]);
        let test2 = route!(router(Get, "path/*/test/no2"), [["test"]]);
        let test3 = route!(router(Get, "path/to/*/*/*"), [[*""], ["test"]]);
        let test4 = route!(router(Get, "path/to"), [[*""], ["test"]]);

        check!(router(Get, "path/to/test1") => Some(&test1));
        check!(router(Get, "path/for/test/no2") => Some(&test2));
        check!(router(Get, "path/to/test1/no/test3") => Some(&test3));
        check!(router(Get, "path/to/test1/no/test3/again") => Some(&test3));
        check!(router(Get, "path/to") => Some(&test4));
        check!(router(Get, "path") => None);
    }

    #[test]
    fn several_wildcards_routes_no_hyperlinks() {
        let mut router = TestRouter::new();

        let test1 = route!(router(Get, "path/to/*tail"));
        let test2 = route!(router(Get, "path/*middle/test/no2"));
        let test3 = route!(router(Get, "path/to/*a/*b/*c"));

        check!(router(Get, "path/to/test1") => Some(&test1), {"tail" => "test1"});
        check!(router(Get, "path/for/test/no2") => Some(&test2), {"middle" => "for"});
        check!(router(Get, "path/to/test1/no/test3") => Some(&test3), {"a" => "test1", "b" => "no", "c" => "test3"});
        check!(router(Get, "path/to/test1/no/test3/again") => Some(&test3), {"a" => "test1", "b" => "no", "c" => "test3/again"});
        check!(router(Get, "path/to") => None);
    }

    #[test]
    fn route_formats() {
        let mut router = TestRouter::new();
        router.find_hyperlinks = true;

        let test1 = route!(router(Get, "/"), [["path"]]);
        let test2 = route!(router(Get, "/path/to/test/no2"));
        let test3 = route!(router(Get, "path/to/test3/"), [["again"]]);
        let test3_again = route!(router(Get, "/path/to/test3/again/"));

        check!(router(Get, "") => Some(&test1));
        check!(router(Get, "path/to/test/no2/") => Some(&test2));
        check!(router(Get, "path/to/test3") => Some(&test3));
        check!(router(Get, "/path/to/test3/again") => Some(&test3_again));
        check!(router(Get, "//path/to/test3") => None);
    }

    #[test]
    fn http_methods() {
        let mut router = TestRouter::new();
        router.find_hyperlinks = true;

        let get = route!(router(Get, "/"), [[./Post], [./Delete], [./Put]]);
        let post = route!(router(Post, "/"), [[./Get], [./Delete], [./Put]]);
        let delete = route!(router(Delete, "/"), [[./Get], [./Post], [./Put]]);
        let put = route!(router(Put, "/"), [[./Get], [./Post], [./Delete]]);

        check!(router(Get, "/") => Some(&get));
        check!(router(Post, "/") => Some(&post));
        check!(router(Delete, "/") => Some(&delete));
        check!(router(Put, "/") => Some(&put));
        check!(router(Head, "/") => None);
    }

    #[test]
    fn merge_routers() {
        let mut router1 = TestRouter::new();
        router1.find_hyperlinks = true;

        let router1_test1 = route!(router1(Get, ""), [["path"]]);
        let router1_test2 = route!(router1(Get, "path/to/test/no2"));
        let router1_test3 = route!(router1(Get, "path/to/test1/no/test3"), [[./Post]]);

        let mut router2 = TestRouter::new();

        let router2_test1 = route!(router2(Get, ""), [["test"], ["test1"]]);
        let router2_test5 = route!(router2(Get, "test/no5"));
        let router2_test3_post = route!(router2(Post, "test1/no/test3"), [[./Get]]);

        router1.insert_router("path/to", router2);

        check!(router1(Get, "") => Some(&router1_test1));
        check!(router1(Get, "path/to/test/no2") => Some(&router1_test2));
        check!(router1(Get, "path/to/test1/no/test3") => Some(&router1_test3));
        check!(router1(Post, "path/to/test1/no/test3") => Some(&router2_test3_post));
        check!(router1(Get, "path/to/test/no5") => Some(&router2_test5));
        check!(router1(Get, "path/to") => Some(&router2_test1));
    }


    #[test]
    fn merge_routers_variables() {
        let mut router1 = TestRouter::new();
        router1.find_hyperlinks = true;

        let test1 = route!(router1(Get, ":a/:b/:c"), [["test"]]);

        let mut router2 = TestRouter::new();

        let test2 = route!(router2(Get, ":b/:c/test"));

        router1.insert_router(":a", router2);

        check!(router1(Get, "path/to/test1") => Some(&test1), {"a" => "path", "b" => "to", "c" => "test1"});
        check!(router1(Get, "path/to/test1/test") => Some(&test2), {"a" => "path", "b" => "to", "c" => "test1"});
    }


    #[test]
    fn merge_routers_wildcard() {
        let mut router1 = TestRouter::new();
        router1.find_hyperlinks = true;

        let test2 = route!(router1(Get, "path/to"), [["test1"]]);

        let mut router2 = TestRouter::new();

        let test1 = route!(router2(Get, "*/test1"), [["test1"]]);

        router1.insert_router("path", router2);

        check!(router1(Get, "path/to/test1") => Some(&test1));
        check!(router1(Get, "path/to/same/test1") => Some(&test1));
        check!(router1(Get, "path/to/the/same/test1") => Some(&test1));
        check!(router1(Get, "path/to") => Some(&test2));
        check!(router1(Get, "path") => None);
    }

    #[test]
    fn prefix_router_variables() {
        let mut router = TestRouter::new();
        router.find_hyperlinks = true;

        let test2 = route!(router(Get, ":b/:c"), [["test"]]);
        let test1 = route!(router(Get, ":b/:c/test"));

        router.prefix(":a");

        check!(router(Get, "path/to/test1") => Some(&test2), {"a" => "path", "b" => "to", "c" => "test1"});
        check!(router(Get, "path/to/test1/test") => Some(&test1), {"a" => "path", "b" => "to", "c" => "test1"});
    }

    #[test]
    fn prefix_router_wildcard() {
        let mut router = TestRouter::new();
        router.find_hyperlinks = true;

        let test2 = route!(router(Get, "path/to"), [["test1"]]);
        let test1 = route!(router(Get, "*/test1"), [["test1"]]);

        router.prefix(":a");

        check!(router(Get, "a/path/to/test1") => Some(&test1), {"a" => "a"});
        check!(router(Get, "a/path/to/same/test1") => Some(&test1), {"a" => "a"});
        check!(router(Get, "a/path/to/the/same/test1") => Some(&test1), {"a" => "a"});
        check!(router(Get, "a/path/to") => Some(&test2), {"a" => "a"});
        check!(router(Get, "a/path") => None);
    }

   //  #[bench]
   //  #[cfg(feature = "benchmark")]
   //  fn search_speed(b: &mut Bencher) {
   //      let routes: Vec<(_, _, TestHandler)> = vec![
   //          (Get, "path/to/test1", "test 1".into()),
   //          (Get, "path/to/test/no2", "test 1".into()),
   //          (Get, "path/to/test1/no/test3", "test 1".into()),
   //          (Get, "path/to/other/test1", "test 1".into()),
   //          (Get, "path/to/test/no2/again", "test 1".into()),
   //          (Get, "other/path/to/test1/no/test3", "test 1".into()),
   //          (Get, "path/to/test1", "test 1".into()),
   //          (Get, "path/:a/test/no2", "test 1".into()),
   //          (Get, "path/to/:b/:c/:a", "test 1".into()),
   //          (Get, "path/to/*a", "test 1".into()),
   //          (Get, "path/to/*a/other", "test 1".into())
   //      ];

   //      let paths = [
   //          "path/to/test1",
   //          "path/to/test/no2",
   //          "path/to/test1/no/test3",
   //          "path/to/other/test1",
   //          "path/to/test/no2/again",
   //          "other/path/to/test1/no/test3",
   //          "path/a/test1",
   //          "path/a/test/no2",
   //          "path/to/b/c/a",
   //          "path/to/test1/no",
   //          "path/to",
   //          "path/to/test1/nothing/at/all"
   //      ];

   //      let router = routes.into_iter().collect::<TestRouter>();
   //      let mut counter = 0;

   //      b.iter(|| {
   //          router.find(&Get, &mut (paths[counter].into()));
   //          counter = (counter + 1) % paths.len()
   //      });
   //  }


   //  #[bench]
   //  #[cfg(feature = "benchmark")]
   //  fn wildcard_speed(b: &mut Bencher) {
   //      let routes: Vec<(_, _, TestHandler)> = vec![
   //          (Get, "*/to/*/*/a", "test 1".into())
   //      ];

   //      let paths = [
   //          "path/to/a",
   //          "path/to/test/a",
   //          "path/to/test1/no/a",
   //          "path/to/other/a",
   //          "path/to/test/no2/a",
   //          "other/path/to/test1/no/a",
   //          "path/a/a",
   //          "path/a/test/a",
   //          "path/to/b/c/a",
   //          "path/to/test1/a",
   //          "path/a",
   //          "path/to/test1/nothing/at/all/and/all/and/all/and/a"
   //      ];

   //      let router = routes.into_iter().collect::<TestRouter>();
   //      let mut counter = 0;

   //      b.iter(|| {
   //          router.find(&Get, &mut (paths[counter].into()));
   //          counter = (counter + 1) % paths.len()
   //      });
   //  }
}
