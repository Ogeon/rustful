//! A tree shaped router that selects handlers using paths.

use std::collections::HashMap;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::iter::{Iterator, IntoIterator, FromIterator};
use std::borrow::Cow;
use hyper::method::Method;

use context::{MaybeUtf8Owned, MaybeUtf8Slice};
use context::hypermedia::{Link, LinkSegment, SegmentType};
use handler::{HandleRequest, Environment, MethodRouter, Variables, Build, FromHandler, ApplyContext, Merge, BuilderContext, VariableNames};
use handler::routing::Route;
use StatusCode;

use self::Branch::{Static, Variable, Wildcard};

#[derive(PartialEq)]
enum Branch {
    Static,
    Variable,
    Wildcard
}

/// A tree shaped router that selects handlers using paths.
///
/// Each tree node stores an other router of type `T`, which has to implement
/// `Default` to allow "empty" nodes, and a number of child references. The
/// requested path must always be exhausted before sub-routers are searched, so
/// there is no point in storing other path based routers in a `TreeRouter`.
///
/// The `TreeRouter` has support for shallow hyperlinks to children, siblings,
/// cousins, and so forth. The use of variable sequences complicates this
/// process and may cause confusing results in certain situations. The
/// hyperlinks may or may not point to a handler.
///
/// Hyperlinks has to be activated by setting `find_hyperlinks` to  `true`.
#[derive(Clone)]
pub struct TreeRouter<T> {
    item: T,
    static_routes: HashMap<MaybeUtf8Owned, TreeRouter<T>>,
    variable_route: Option<Box<TreeRouter<T>>>,
    wildcard_route: Option<Box<TreeRouter<T>>>,
    /// Should the router search for hyperlinks? Setting this to `true` may
    /// slow down endpoint search, but enables hyperlinks.
    pub find_hyperlinks: bool
}

impl<T: Default> TreeRouter<T> {
    /// Creates an empty `TreeRouter`.
    pub fn new() -> TreeRouter<T> {
        TreeRouter::default()
    }
}

impl<T> TreeRouter<T> {
    /// Creates a `TreeRouter` with only a root handler.
    ///
    /// ```
    /// use rustful::{Context, Response};
    /// use rustful::handler::TreeRouter;
    ///
    /// fn handler(_context: Context, response: Response) {
    ///     response.send("Hello world!");
    /// }
    ///
    /// let router = TreeRouter::with_handler(handler);
    /// ```
    pub fn with_handler(handler: T) -> TreeRouter<T> {
        TreeRouter {
            item: handler,
            static_routes: HashMap::new(),
            variable_route: None,
            wildcard_route: None,
            find_hyperlinks: false
        }
    }

    /// Build the router and its children using a chaninable API.
    ///
    /// ```
    /// use rustful::{Context, Response};
    /// use rustful::handler::TreeRouter;
    ///
    /// fn handler(_context: Context, response: Response) {
    ///     response.send("Hello world!");
    /// }
    ///
    /// let mut router = TreeRouter::<Option<fn(Context, Response)>>::new();
    /// router.build().on_path("hello/world", handler);
    /// ```
    pub fn build(&mut self) -> Builder<T> {
        self.get_builder(BuilderContext::new())
    }

    // Tries to find a router matching the key or inserts a new one if none exists.
    fn find_or_insert_router<'a, F: FnOnce() -> T>(&'a mut self, key: &[u8], create_handler: F) -> &'a mut TreeRouter<T> {
        if let Some(&b'*') = key.get(0) {
            if self.wildcard_route.is_none() {
                self.wildcard_route = Some(Box::new(TreeRouter::with_handler(create_handler())));
            }
            &mut **self.wildcard_route.as_mut().unwrap()
        } else if let Some(&b':') = key.get(0) {
            if self.variable_route.is_none() {
                self.variable_route = Some(Box::new(TreeRouter::with_handler(create_handler())));
            }
            &mut **self.variable_route.as_mut().unwrap()
        } else {
            match self.static_routes.entry(key.to_owned().into()) {
                Occupied(entry) => entry.into_mut(),
                Vacant(entry) => entry.insert(TreeRouter::with_handler(create_handler()))
            }
        }
    }
}

impl<T: FromHandler<H> + ApplyContext, D: AsRef<[u8]>, H> FromIterator<(Method, D, H)> for TreeRouter<MethodRouter<Variables<T>>> {
    /// Create a `DefaultRouter` from a collection of routes.
    ///
    /// ```
    /// extern crate rustful;
    /// use rustful::Method::Get;
    /// # use rustful::{Handler, Context, Response, DefaultRouter};
    ///
    /// # struct ExampleHandler;
    /// # impl Handler for ExampleHandler {
    /// #     fn handle(&self, _: Context, _: Response){}
    /// # }
    /// # fn main() {
    /// # let about_us = ExampleHandler;
    /// # let show_user = ExampleHandler;
    /// # let list_users = ExampleHandler;
    /// # let show_product = ExampleHandler;
    /// # let list_products = ExampleHandler;
    /// # let show_error = ExampleHandler;
    /// # let show_welcome = ExampleHandler;
    /// let routes = vec![
    ///     (Get, "/about", about_us),
    ///     (Get, "/users", list_users),
    ///     (Get, "/users/:id", show_user),
    ///     (Get, "/products", list_products),
    ///     (Get, "/products/:id", show_product),
    ///     (Get, "/*", show_error),
    ///     (Get, "/", show_welcome)
    /// ];
    ///
    /// let router: DefaultRouter<ExampleHandler> = routes.into_iter().collect();
    /// # }
    /// ```
    fn from_iter<I: IntoIterator<Item=(Method, D, H)>>(iterator: I) -> TreeRouter<MethodRouter<Variables<T>>> {
        let mut root = TreeRouter::<MethodRouter<Variables<T>>>::new();

        for (method, route, item) in iterator {
            root.build().path(route).then().on(method, item);
        }

        root
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
                        return Err(environment);
                    } else {
                        return Ok(());
                    }
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

        if matches.is_empty() {
            environment.response.set_status(StatusCode::NotFound);
        } else if self.find_hyperlinks {
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

impl<T: Default> Default for TreeRouter<T> {
    fn default() -> TreeRouter<T> {
        TreeRouter::with_handler(T::default())
    }
}

impl<'a, T: 'a> Build<'a> for TreeRouter<T> {
    type Builder = Builder<'a, T>;

    fn get_builder(&'a mut self, mut context: BuilderContext) -> Builder<'a, T> {
        Builder {
            node: self,
            variables: Cow::Owned(context.remove::<VariableNames>().unwrap_or_default().0),
            context: Cow::Owned(context)
        }
    }
}

impl<T: FromHandler<H>, H> FromHandler<H> for TreeRouter<T> {
    fn from_handler(context: BuilderContext, handler: H) -> TreeRouter<T> {
        TreeRouter::with_handler(T::from_handler(context, handler))
    }
}

impl<T: ApplyContext> ApplyContext for TreeRouter<T> {
    fn apply_context(&mut self, mut context: BuilderContext) {
        if let Some(VariableNames(variables)) = context.remove() {
            let mut variable_context = BuilderContext::new();
            variable_context.insert(VariableNames(variables.clone()));

            self.item.prepend_context(variable_context);
            self.item.apply_context(context.clone());

            context.insert(VariableNames(variables));
        }


        for (_, mut node) in &mut self.static_routes {
            node.apply_context(context.clone());
        }

        if let Some(ref mut node) = self.variable_route {
            node.apply_context(context.clone())
        }

        if let Some(ref mut node) = self.wildcard_route {
            node.apply_context(context)
        }
    }

    fn prepend_context(&mut self, context: BuilderContext) {
        self.item.prepend_context(context.clone());

        for (_, node) in &mut self.static_routes {
            node.prepend_context(context.clone());
        }

        if let Some(ref mut node) = self.variable_route {
            node.prepend_context(context.clone())
        }

        if let Some(ref mut node) = self.wildcard_route {
            node.prepend_context(context)
        }
    }
}

impl<T: Merge> Merge for TreeRouter<T> {
    fn merge(&mut self, other: TreeRouter<T>) {
        self.item.merge(other.item);

        for (key, other_node) in other.static_routes {
            println!("merging {:}", key.as_utf8_lossy());
            match self.static_routes.entry(key) {
                Vacant(entry) => { entry.insert(other_node); },
                Occupied(mut entry) => entry.get_mut().merge(other_node)
            }
        }

        if let Some(ref mut this_node) = self.variable_route {
            if let Some(other_node) = other.variable_route {
                this_node.merge(*other_node);
            }
        } else {
            self.variable_route = other.variable_route;
        }

        if let Some(ref mut this_node) = self.wildcard_route {
            if let Some(other_node) = other.wildcard_route {
                this_node.merge(*other_node);
            }
        } else {
            self.wildcard_route = other.wildcard_route;
        }
    }
}

/// A builder for a `TreeRouter`.
pub struct Builder<'a, T: 'a> {
    node: &'a mut TreeRouter<T>,
    variables: Cow<'a, [MaybeUtf8Owned]>,
    context: Cow<'a, BuilderContext>
}

impl<'a, T: Default + ApplyContext> Builder<'a, T> {
    /// Add a path to the router and keep building the resulting node.
    ///
    /// ```
    /// use rustful::{Context, Response};
    /// use rustful::handler::TreeRouter;
    ///
    /// fn handler1(_context: Context, response: Response) {
    ///     response.send("Hello world 1!");
    /// }
    ///
    /// fn handler2(_context: Context, response: Response) {
    ///     response.send("Hello world 2!");
    /// }
    ///
    /// let mut router = TreeRouter::<Option<fn(Context, Response)>>::new();
    /// router.build().path("hello/world").many(|mut node| {
    ///     node.on_route("1", handler1);
    ///     node.on_route("2", handler2);
    /// });
    /// ```
    pub fn path<'b, S: AsRef<[u8]>>(&'b mut self, path: S) -> Builder<'b, T> {
        let mut variables = self.variables.clone();
        let context = self.context.clone();

        let node = path.as_ref().segments().fold(&mut *self.node, |node, segment| {
            if segment[0] == b':' || segment[0] == b'*' {
                variables.to_mut().push(segment[1..].to_owned().into());
            }

            node.find_or_insert_router(segment, || {
                let mut new_context = context.clone().into_owned();
                new_context.insert(VariableNames(variables.clone().into_owned()));
                let mut handler = T::default();
                handler.apply_context(new_context);
                handler
            })
        });

        Builder {
            node: node,
            variables: variables,
            context: self.context.clone()
        }
    }

    /// Add a handler at the end of a path and keep building the resulting node.
    ///
    /// ```
    /// use rustful::{Context, Response};
    /// use rustful::handler::TreeRouter;
    ///
    /// fn handler(_context: Context, response: Response) {
    ///     response.send("Hello world!");
    /// }
    ///
    /// fn handler1(_context: Context, response: Response) {
    ///     response.send("Hello world 1!");
    /// }
    ///
    /// fn handler2(_context: Context, response: Response) {
    ///     response.send("Hello world 2!");
    /// }
    ///
    /// let mut router = TreeRouter::<Option<fn(Context, Response)>>::new();
    /// router.build().on_path("hello/world", handler).many(|mut node| {
    ///     node.on_route("1", handler1);
    ///     node.on_route("2", handler2);
    /// });
    /// ```
    pub fn on_path<'b, S: AsRef<[u8]>, H>(&'b mut self, path: S, handler: H) -> Builder<'b, T> where T: FromHandler<H> {
        let mut variables = self.variables.clone();
        let context = self.context.clone();

        let node = path.as_ref().segments().fold(&mut *self.node, |node, segment| {
            if segment[0] == b':' || segment[0] == b'*' {
                variables.to_mut().push(segment[1..].to_owned().into());
            }

            node.find_or_insert_router(segment, || {
                let mut new_context = context.clone().into_owned();
                new_context.insert(VariableNames(variables.clone().into_owned()));
                let mut handler = T::default();
                handler.apply_context(new_context);
                handler
            })
        });

        let mut new_context = context.clone().into_owned();
        new_context.insert(VariableNames(variables.clone().into_owned()));
        node.item = T::from_handler(new_context, handler);

        Builder {
            node: node,
            variables: variables,
            context: context
        }
    }
}

impl<'a, T> Builder<'a, T> {
    /// Perform more than one operation on this builder.
    ///
    /// ```
    /// use rustful::{Context, Response};
    /// use rustful::handler::TreeRouter;
    ///
    /// fn handler1(_context: Context, response: Response) {
    ///     response.send("Hello world 1!");
    /// }
    ///
    /// fn handler2(_context: Context, response: Response) {
    ///     response.send("Hello world 2!");
    /// }
    ///
    /// let mut router = TreeRouter::<Option<fn(Context, Response)>>::new();
    /// router.build().many(|mut node| {
    ///     node.on_route("1", handler1);
    ///     node.on_route("2", handler2);
    /// });
    /// ```
    pub fn many<F: FnOnce(&mut Builder<'a, T>)>(&mut self, build: F) -> &mut Builder<'a, T> {
        build(self);
        self
    }

    /// Add a handler at the end of a route segment and keep building the resulting node.
    ///
    /// ```
    /// use rustful::{Context, Response};
    /// use rustful::handler::TreeRouter;
    ///
    /// fn handler(_context: Context, response: Response) {
    ///     response.send("Hello world!");
    /// }
    ///
    /// fn handler1(_context: Context, response: Response) {
    ///     response.send("Hello world 1!");
    /// }
    ///
    /// fn handler2(_context: Context, response: Response) {
    ///     response.send("Hello world 2!");
    /// }
    ///
    /// let mut router = TreeRouter::<Option<fn(Context, Response)>>::new();
    /// router.build().on_route("hello_world", handler).many(|mut node| {
    ///     node.on_route("1", handler1);
    ///     node.on_route("2", handler2);
    /// });
    /// ```
    pub fn on_route<'b, S: AsRef<[u8]>, H>(&'b mut self, label: S, handler: H) -> Builder<'b, T> where T: FromHandler<H> {
        let mut variables = self.variables.clone();
        let context = self.context.clone();

        let mut label_iter = label.as_ref().segments();
        let label = if let Some(label) = label_iter.next() {
            label
        } else {
            return Builder {
                node: self.node,
                variables: self.variables.clone(),
                context: self.context.clone()
            };
        };

        if label_iter.next().is_some() {
            panic!("expected at most a single label, but got a longer path");
        }

        if label[0] == b':' || label[0] == b'*' {
            variables.to_mut().push(label[1..].to_owned().into());
        }

        let node = self.node.find_or_insert_router(label, || {
            let mut new_context = context.clone().into_owned();
            new_context.insert(VariableNames(variables.clone().into_owned()));
            T::from_handler(new_context, handler)
        });

        Builder {
            node: node,
            variables: variables,
            context: self.context.clone()
        }
    }


    /// Try to get a node at the end of an existing path.
    ///
    /// ```
    /// use rustful::{Context, Response};
    /// use rustful::handler::TreeRouter;
    ///
    /// fn handler(_context: Context, response: Response) {
    ///     response.send("Hello world!");
    /// }
    ///
    /// let mut router = TreeRouter::<Option<fn(Context, Response)>>::new();
    ///
    /// router.build().path("hello/world");
    /// router.build().get_path("hello/world").expect("where did it go?!").handler(handler);
    /// ```
    pub fn get_path<'b, S: AsRef<[u8]>>(&'b mut self, path: S) -> Option<Builder<'b, T>> {
        let mut variables = self.variables.clone();

        let node = path.as_ref().segments().fold(Some(&mut *self.node), |maybe_node, segment| {
            maybe_node.and_then(|node| {
                if let Some(&b'*') = segment.get(0) {
                    variables.to_mut().push(segment[1..].to_owned().into());
                    node.wildcard_route.as_mut().map(|node| &mut **node)
                } else if let Some(&b':') = segment.get(0) {
                    variables.to_mut().push(segment[1..].to_owned().into());
                    node.variable_route.as_mut().map(|node| &mut **node)
                } else {
                    node.static_routes.get_mut(segment)
                }
            })
        });

        let new_context = self.context.clone();
        node.map(|node| Builder {
            node: node,
            variables: variables,
            context: new_context
        })
    }

    /// Set or replace the handler at the current node.
    pub fn handler<'b, H>(&'b mut self, handler: H) -> Builder<'b, T> where T: FromHandler<H> {
        let mut new_context = self.context.clone().into_owned();
        new_context.insert(VariableNames(self.variables.clone().into_owned()));
        self.node.item = T::from_handler(new_context, handler);

        Builder {
            node: self.node,
            variables: self.variables.clone(),
            context: self.context.clone()
        }
    }
}

impl<'a: 'b, 'b, T: Build<'b>> Builder<'a, T> {
    /// Build the handler at the current node.
    ///
    /// ```
    /// use rustful::{Context, Response};
    /// use rustful::handler::{TreeRouter, MethodRouter};
    ///
    /// fn handler(_context: Context, response: Response) {
    ///     response.send("Hello world!");
    /// }
    ///
    /// let mut router = TreeRouter::<MethodRouter<fn(Context, Response)>>::new();
    /// router.build().path("hello/world").then().on_get(handler);
    /// ```
    pub fn then(&'b mut self) -> T::Builder {
        let mut new_context = self.context.clone().into_owned();
        new_context.insert(VariableNames(self.variables.clone().into_owned()));
        self.node.item.get_builder(new_context)
    }
}

impl<'a, T: Merge + ApplyContext> Builder<'a, T> {
    ///Move handlers from another router into this, overwriting conflicting handlers and properties.
    pub fn merge(&mut self, mut other: TreeRouter<T>) -> &mut Builder<'a, T> {
        let mut new_context = self.context.clone().into_owned();
        new_context.insert(VariableNames(self.variables.clone().into_owned()));
        other.apply_context(new_context);

        self.node.merge(other);

        self
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

                $router.build().path($path).then().on($method, handler);
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
                    if let Err(environment) = result {
                        assert!(environment.response.status().is_client_error());
                    } else {
                        panic!("expected the environment to be returned");
                    }
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

        router1.build().path("path/to").merge(router2);

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

        router1.build().path(":a").merge(router2);

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

        router1.build().path("path").merge(router2);

        check!(router1(Get, "path/to/test1") => Some(&test1));
        check!(router1(Get, "path/to/same/test1") => Some(&test1));
        check!(router1(Get, "path/to/the/same/test1") => Some(&test1));
        check!(router1(Get, "path/to") => Some(&test2));
        check!(router1(Get, "path") => None);
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
