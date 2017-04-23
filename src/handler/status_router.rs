//! A router that selects an item from an HTTP status code.

use std::collections::hash_map::{HashMap, Entry};

use context::hypermedia::Link;
use StatusCode;
use handler::{HandleRequest, Environment, Build, ApplyContext, Merge, FromHandler, BuilderContext};

/// A router that selects an item from an HTTP status code.
///
/// It does neither alter nor preserve the status code of the response.
#[derive(Clone)]
pub struct StatusRouter<T> {
    handlers: HashMap<StatusCode, T>
}

impl<T> StatusRouter<T> {
    /// Create an empty `StatusRouter`.
    pub fn new() -> StatusRouter<T> {
        StatusRouter::default()
    }

    /// Build the router and its children using a chaninable API.
    ///
    /// ```
    /// use rustful::{Context, Response, StatusCode, StatusRouter};
    ///
    /// fn on_404(context: Context, response: Response) {
    ///     response.send(format!("{} was not found.", context.uri_path));
    /// }
    ///
    /// fn on_500(_: Context, response: Response) {
    ///     response.send("Looks like you found a bug");
    /// }
    /// let mut status_router = StatusRouter::<fn(Context, Response)>::new();
    ///
    /// status_router.build().many(|mut status_router|{
    ///     status_router.on(StatusCode::NotFound, on_404 as fn(Context, Response));
    ///     status_router.on(StatusCode::InternalServerError, on_500);
    /// });
    /// ```
    pub fn build(&mut self) -> Builder<T> {
        self.get_builder(BuilderContext::new())
    }

    /// Insert a handler that will listen for a specific status code.
    ///
    /// ```
    /// use rustful::{Context, Response, StatusCode, StatusRouter};
    /// use rustful::handler::MethodRouter;
    ///
    /// let on_not_found = MethodRouter::<fn(Context, Response)>::new();
    /// //Fill on_not_found with handlers...
    ///
    /// let mut status_router = StatusRouter::new();
    /// status_router.insert(StatusCode::NotFound, on_not_found);
    /// ```
    pub fn insert(&mut self, status: StatusCode, handler: T) {
        self.handlers.insert(status, handler);
    }
}

impl<T: HandleRequest> HandleRequest for StatusRouter<T> {
    fn handle_request<'a, 'b, 'l, 'g>(&self, environment: Environment<'a, 'b, 'l, 'g>) -> Result<(), Environment<'a, 'b, 'l, 'g>> {
        if let Some(handler) = self.handlers.get(&environment.response.status()) {
            handler.handle_request(environment)
        } else {
            Err(environment)
        }
    }

    fn hyperlinks<'a>(&'a self, base: Link<'a>) -> Vec<Link<'a>> {
        self.handlers.iter().flat_map(|(_, item)| {
            item.hyperlinks(base.clone())
        }).collect()
    }
}

impl<T> Default for StatusRouter<T> {
    fn default() -> StatusRouter<T> {
        StatusRouter {
            handlers: HashMap::new(),
        }
    }
}

impl<'a, T: 'a> Build<'a> for StatusRouter<T> {
    type Builder = Builder<'a, T>;

    fn get_builder(&'a mut self, context: BuilderContext) -> Builder<'a, T> {
        Builder {
            router: self,
            context: context
        }
    }
}

impl<T: ApplyContext> ApplyContext for StatusRouter<T> {
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

impl<T: Merge> Merge for StatusRouter<T> {
    fn merge(&mut self, other: StatusRouter<T>) {
        for (status, handler) in other.handlers {
            match self.handlers.entry(status) {
                Entry::Vacant(entry) => { entry.insert(handler); },
                Entry::Occupied(mut entry) => entry.get_mut().merge(handler),
            }
        }
    }
}

/// A builder for a `StatusRouter`.
pub struct Builder<'a, T: 'a> {
    router: &'a mut StatusRouter<T>,
    context: BuilderContext
}

impl<'a, T> Builder<'a, T> {
    /// Perform more than one operation on this builder.
    ///
    /// ```
    /// use rustful::{Context, Response, StatusCode, StatusRouter};
    ///
    /// fn on_404(context: Context, response: Response) {
    ///     response.send(format!("{} was not found.", context.uri_path));
    /// }
    ///
    /// fn on_500(_: Context, response: Response) {
    ///     response.send("Looks like you found a bug");
    /// }
    /// let mut status_router = StatusRouter::<fn(Context, Response)>::new();
    ///
    /// status_router.build().many(|mut status_router|{
    ///     status_router.on(StatusCode::NotFound, on_404 as fn(Context, Response));
    ///     status_router.on(StatusCode::InternalServerError, on_500);
    /// });
    /// ```
    pub fn many<F: FnOnce(&mut Builder<'a, T>)>(&mut self, build: F) -> &mut Builder<'a, T> {
        build(self);
        self
    }

    /// Insert a handler that will listen for a specific status code.
    ///
    /// ```
    /// use rustful::{Context, Response, StatusCode, StatusRouter};
    ///
    /// fn on_404(context: Context, response: Response) {
    ///     response.send(format!("{} was not found.", context.uri_path));
    /// }
    ///
    /// let mut status_router = StatusRouter::<fn(Context, Response)>::new();
    /// status_router.build().on(StatusCode::NotFound, on_404 as fn(Context, Response));
    /// ```
    pub fn on<H>(&mut self, status: StatusCode, handler: H) where T: FromHandler<H> {
        self.router.handlers.insert(status, T::from_handler(self.context.clone(), handler));
    }
}

impl<'a: 'b, 'b, T: Default + ApplyContext + Build<'b>> Builder<'a, T> {
    /// Build a handler that will listen for a specific status code.
    ///
    /// ```
    /// use rustful::{Context, Response, StatusCode, StatusRouter};
    /// use rustful::handler::MethodRouter;
    ///
    /// fn on_get_404(context: Context, response: Response) {
    ///     response.send(format!("GET {} was not found.", context.uri_path));
    /// }
    ///
    /// let mut status_router = StatusRouter::<MethodRouter<fn(Context, Response)>>::new();
    ///
    /// status_router.build()
    ///     .status(StatusCode::NotFound)
    ///     .on_get(on_get_404 as fn(Context, Response));
    /// ```
    pub fn status(&'b mut self, status: StatusCode) -> T::Builder {
        match self.router.handlers.entry(status) {
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
    pub fn merge(&mut self, mut other: StatusRouter<T>) -> &mut Builder<'a, T> {
        other.apply_context(self.context.clone());
        self.router.merge(other);

        self
    }
}
