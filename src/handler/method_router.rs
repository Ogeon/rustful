//! A router that selects a handler from an HTTP method.

use std::collections::hash_map::{HashMap, Entry};

use {Method, StatusCode};
use context::hypermedia::Link;
use handler::{HandleRequest, Environment, Build, FromHandler, BuilderContext, ApplyContext, Merge};

/// A router that selects a handler from an HTTP method.
///
/// It's a simple mapping between `Method` and a router `T`, while the
/// requested path is ignored. It's therefore a good idea to pair a
/// `MethodRouter` with an exhaustive path router of some sort.
#[derive(Clone)]
pub struct MethodRouter<T> {
    handlers: HashMap<Method, T>,
}

impl<T> MethodRouter<T> {
    /// Create an empty `MethodRouter`.
    pub fn new() -> MethodRouter<T> {
        MethodRouter::default()
    }

    /// Build the router and its children using a chaninable API.
    ///
    /// ```
    /// use rustful::Context;
    /// use rustful::handler::MethodRouter;
    ///
    /// fn get(_context: &mut Context) -> &'static str {
    ///     "A GET request."
    /// }
    ///
    /// fn post(_context: &mut Context) -> &'static str {
    ///     "A POST request."
    /// }
    ///
    /// let mut method_router = MethodRouter::<fn(&mut Context) -> &'static str>::new();
    ///
    /// method_router.build().many(|mut method_router|{
    ///     method_router.on_get(get);
    ///     method_router.on_post(post);
    /// });
    /// ```
    pub fn build(&mut self) -> Builder<T> {
        self.get_builder(BuilderContext::new())
    }

    /// Insert a handler that will listen for a specific status code.
    ///
    /// ```
    /// use rustful::{Context, Method};
    /// use rustful::handler::{MethodRouter, TreeRouter};
    ///
    /// let route_tree = TreeRouter::<Option<fn(&mut Context) -> &'static str>>::new();
    /// //Fill route_tree with handlers...
    ///
    /// let mut method_router = MethodRouter::new();
    /// method_router.insert(Method::Get, route_tree);
    /// ```
    pub fn insert(&mut self, method: Method, handler: T) {
        self.handlers.insert(method, handler);
    }
}

impl<T: HandleRequest> HandleRequest for MethodRouter<T> {
    fn handle_request<'a, 'b, 'l, 'g>(&self, mut environment: Environment<'a, 'b, 'l, 'g>) -> Result<(), Environment<'a, 'b, 'l, 'g>> {
        if let Some(handler) = self.handlers.get(&environment.context.method) {
            handler.handle_request(environment)
        } else {
            environment.response.set_status(StatusCode::MethodNotAllowed);
            Err(environment)
        }
    }

    fn hyperlinks<'a>(&'a self, base: Link<'a>) -> Vec<Link<'a>> {
        self.handlers.iter().flat_map(|(method, handler)| {
            let mut link = base.clone();
            link.method = Some(method.clone());
            handler.hyperlinks(link)
        }).collect()
    }
}

impl<T> Default for MethodRouter<T> {
    fn default() -> MethodRouter<T> {
        MethodRouter {
            handlers: HashMap::new(),
        }
    }
}

impl<'a, T: 'a> Build<'a> for MethodRouter<T> {
    type Builder = Builder<'a, T>;

    fn get_builder(&'a mut self, context: BuilderContext) -> Self::Builder {
        Builder {
            router: self,
            context: context
        }
    }
}

impl<T: ApplyContext> ApplyContext for MethodRouter<T> {
    fn apply_context(&mut self, context: BuilderContext) {
        for (_, handler) in &mut self.handlers {
            handler.apply_context(context.clone());
        }
    }

    fn prepend_context(&mut self, context: BuilderContext) {
        for (_, handler) in &mut self.handlers {
            handler.prepend_context(context.clone());
        }
    }
}

impl<T: Merge> Merge for MethodRouter<T> {
    fn merge(&mut self, other: MethodRouter<T>) {
        println!("merging {:} methods with {:} methods", self.handlers.len(), other.handlers.len());
        for (method, handler) in other.handlers {
            println!("merging {:}", method);
            match self.handlers.entry(method) {
                Entry::Vacant(entry) => { entry.insert(handler); },
                Entry::Occupied(mut entry) => entry.get_mut().merge(handler),
            }
        }
    }
}

/// A builder for a `MethodRouter`.
pub struct Builder<'a, T: 'a> {
    router: &'a mut MethodRouter<T>,
    context: BuilderContext
}

impl<'a, T> Builder<'a, T> {
    /// Perform more than one operation on this builder.
    ///
    /// ```
    /// use rustful::Context;
    /// use rustful::handler::MethodRouter;
    ///
    /// fn get(_context: &mut Context) -> &'static str {
    ///     "A GET request."
    /// }
    ///
    /// fn post(_context: &mut Context) -> &'static str {
    ///     "A POST request."
    /// }
    ///
    /// let mut method_router = MethodRouter::<fn(&mut Context) -> &'static str>::new();
    ///
    /// method_router.build().many(|mut method_router|{
    ///     method_router.on_get(get);
    ///     method_router.on_post(post);
    /// });
    /// ```
    pub fn many<F: FnOnce(&mut Builder<'a, T>)>(&mut self, build: F) -> &mut Builder<'a, T> {
        build(self);
        self
    }

    /// Insert a handler for GET requests.
    ///
    /// ```
    /// use rustful::Context;
    /// use rustful::handler::MethodRouter;
    ///
    /// fn handler(_context: &mut Context) -> &'static str {
    ///     "Hello world!"
    /// }
    ///
    /// let mut method_router = MethodRouter::<fn(&mut Context) -> &'static str>::new();
    ///
    /// method_router.build().on_get(handler);
    /// ```
    pub fn on_get<H>(&mut self, handler: H) where T: FromHandler<H> {
        self.on(Method::Get, handler);
    }

    /// Insert a handler for POST requests. See `on_get` for an example.
    pub fn on_post<H>(&mut self, handler: H) where T: FromHandler<H> {
        self.on(Method::Post, handler);
    }

    /// Insert a handler for PATCH requests. See `on_get` for an example.
    pub fn on_patch<H>(&mut self, handler: H) where T: FromHandler<H> {
        self.on(Method::Patch, handler);
    }

    /// Insert a handler for PUT requests. See `on_get` for an example.
    pub fn on_put<H>(&mut self, handler: H) where T: FromHandler<H> {
        self.on(Method::Put, handler);
    }

    /// Insert a handler for DELETE requests. See `on_get` for an example.
    pub fn on_delete<H>(&mut self, handler: H) where T: FromHandler<H> {
        self.on(Method::Delete, handler);
    }

    /// Insert a handler for OPTION requests. See `on_get` for an example.
    pub fn on_options<H>(&mut self, handler: H) where T: FromHandler<H> {
        self.on(Method::Options, handler);
    }

    /// Insert a handler for HEAD requests. See `on_get` for an example.
    pub fn on_head<H>(&mut self, handler: H) where T: FromHandler<H> {
        self.on(Method::Head, handler);
    }

    /// Insert a handler for CONNECT requests. See `on_get` for an example.
    pub fn on_connect<H>(&mut self, handler: H) where T: FromHandler<H> {
        self.on(Method::Connect, handler);
    }

    /// Insert a handler, similar to `on_get`, but for any HTTP method.
    ///
    /// ```
    /// use rustful::Context;
    /// use rustful::handler::MethodRouter;
    /// # fn choose_a_method() -> rustful::Method { rustful::Method::Get }
    ///
    /// fn handler(_context: &mut Context) -> &'static str {
    ///     "Hello world!"
    /// }
    ///
    /// let mut method_router = MethodRouter::<fn(&mut Context) -> &'static str>::new();
    ///
    /// method_router.build().on(choose_a_method(), handler);
    /// ```
    pub fn on<H>(&mut self, method: Method, handler: H) where T: FromHandler<H> {
        self.router.handlers.insert(method, T::from_handler(self.context.clone(), handler));
    }
}

impl<'a: 'b, 'b, T: Default + ApplyContext + Build<'b>> Builder<'a, T> {
    /// Build a handler and its children, for GET requests.
    ///
    /// ```
    /// use rustful::Context;
    /// use rustful::handler::{MethodRouter, TreeRouter};
    ///
    /// fn handler(_context: &mut Context) -> &'static str {
    ///     "Hello world!"
    /// }
    ///
    /// let mut method_router = MethodRouter::<TreeRouter<Option<fn(&mut Context) -> &'static str>>>::new();
    ///
    /// method_router.build().get().on_path("hello/world", handler);
    /// ```
    pub fn get<H>(&'b mut self) -> T::Builder where T: FromHandler<H> {
        self.method(Method::Get)
    }

    /// Build a handler and its children, for POST requests. See `get` for an example.
    pub fn post<H>(&'b mut self) -> T::Builder where T: FromHandler<H> {
        self.method(Method::Post)
    }

    /// Build a handler and its children, for PATCH requests. See `get` for an example.
    pub fn patch<H>(&'b mut self) -> T::Builder where T: FromHandler<H> {
        self.method(Method::Patch)
    }

    /// Build a handler and its children, for PUT requests. See `get` for an example.
    pub fn put<H>(&'b mut self) -> T::Builder where T: FromHandler<H> {
        self.method(Method::Put)
    }

    /// Build a handler and its children, for DELETE requests. See `get` for an example.
    pub fn delete<H>(&'b mut self) -> T::Builder where T: FromHandler<H> {
        self.method(Method::Delete)
    }

    /// Build a handler and its children, for OPTIONS requests. See `get` for an example.
    pub fn options<H>(&'b mut self) -> T::Builder where T: FromHandler<H> {
        self.method(Method::Options)
    }

    /// Build a handler and its children, for HEAD requests. See `get` for an example.
    pub fn head<H>(&'b mut self) -> T::Builder where T: FromHandler<H> {
        self.method(Method::Head)
    }

    /// Build a handler and its children, for CONNECT requests. See `get` for an example.
    pub fn connect<H>(&'b mut self) -> T::Builder where T: FromHandler<H> {
        self.method(Method::Connect)
    }

    /// Build a handler and its children, similar to `get`, but for any HTTP method.
    ///
    /// ```
    /// use rustful::Context;
    /// use rustful::handler::{MethodRouter, TreeRouter};
    /// # fn choose_a_method() -> rustful::Method { rustful::Method::Get }
    ///
    /// fn handler(_context: &mut Context) -> &'static str {
    ///     "Hello world!"
    /// }
    ///
    /// let mut method_router = MethodRouter::<TreeRouter<Option<fn(&mut Context) -> &'static str>>>::new();
    ///
    /// method_router.build()
    ///     .method(choose_a_method())
    ///     .on_path("hello/world", handler);
    /// ```
    pub fn method<H>(&'b mut self, method: Method) -> T::Builder where T: FromHandler<H> {
        match self.router.handlers.entry(method) {
            Entry::Occupied(entry) => entry.into_mut().get_builder(self.context.clone()),
            Entry::Vacant(entry) => {
                let mut handler = T::default();
                handler.apply_context(self.context.clone());
                entry.insert(handler).get_builder(self.context.clone())
            }
        }
    }
}

impl<'a, T: Merge + ApplyContext> Builder<'a, T> {
    ///Move handlers from another router into this, overwriting conflicting handlers and properties.
    pub fn merge(&mut self, mut other: MethodRouter<T>) -> &mut Builder<'a, T> {
        other.apply_context(self.context.clone());
        self.router.merge(other);

        self
    }
}
