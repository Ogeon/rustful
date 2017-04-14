//!Request handlers and routers.
//!
//!# Building Routers
//!
//!Rustful provides a tree structured all-round router called `DefaultRouter`,
//!but any other type of router can be used, as long as it implements the
//![`HandleRequest`][handle_request] trait. Implementing the [`Build`][build]
//!trait will also make it compatible with router builders:
//!
//!```
//!extern crate rustful;
//!use rustful::DefaultRouter;
//!# use rustful::{Handler, Context, Response};
//!
//!# struct ExampleHandler;
//!# impl Handler for ExampleHandler {
//!#     fn handle(&self, _: Context, _: Response){}
//!# }
//!# fn main() {
//!# let about_us = ExampleHandler;
//!# let show_user = ExampleHandler;
//!# let list_users = ExampleHandler;
//!# let show_product = ExampleHandler;
//!# let list_products = ExampleHandler;
//!# let show_error = ExampleHandler;
//!# let show_welcome = ExampleHandler;
//!let mut router = DefaultRouter::<ExampleHandler>::new();
//!router.build().many(|mut node| {
//!    node.then().on_get(show_welcome);
//!    node.path("about").then().on_get(about_us);
//!    node.path("users").many(|mut node| {
//!        node.then().on_get(list_users);
//!        node.path(":id").then().on_get(show_user);
//!    });
//!    node.path("products").many(|mut node| {
//!        node.then().on_get(list_products);
//!        node.path(":id").then().on_get(show_product);
//!    });
//!    node.path("*").then().on_get(show_error);
//!});
//!# }
//!```
//!
//!#Variables
//!
//!Routes in the [`TreeRouter`][tree_router] may contain variables, that are
//!useful for capturing parts of the requested path as input to the handler.
//!The syntax for a variable is simply an indicator character (`:` or `*`)
//!followed by a label. Variables without labels are also valid, but their
//!values will be discarded.
//!
//!##Variable Segments (:label)
//!
//!A variable segment will match a single arbitrary segment. They are probably
//!the most commonly used variables and may, for example, be used to select a
//!blog post: `"posts/:year/:month/:day/:title_slug"`.
//!
//!```text
//!pattern = "a/:v/b"
//!"a/c/b" -> v = "c"
//!"a/c/d/b" -> no match
//!"a/b" -> no match
//!"a/c/b/d" -> no match
//!```
//!
//!##Variable Sequences (*label)
//!
//!A variable sequence is similar to a variable segment, but with the
//!difference that it may consume multiple segments until the rest of the path
//!gives a match. An example use case is a route for downloadable files that
//!may be arranged in arbitrary directories: `"downloads/*file_path"`.
//!
//!```text
//!pattern = "a/*v/b"
//!"a/c/b" -> v = "c"
//!"a/c/d/b" -> v = "c/d"
//!"a/b" -> no match
//!"a/c/b/d" -> no match
//!```
//!
//!```text
//!pattern = "a/b/*v"
//!"a/b/c" -> v = "c"
//!"a/b/c/d" -> v = "c/d"
//!"a/b" -> no match
//!```
//!
//!# Router Composition
//!
//!The default router is actually a composition of three routers:
//![`TreeRouter`][tree_router], [`MethodRouter`][method_router] and
//![`Variables`][variables]. They come together as the type
//!`DefaultRouter<T>`, so no need to write it all out in most of the cases.
//!
//!There may, however, be cases where you want something else. What if you
//!don't care about the HTTP method? Maybe your handler takes care of that
//!internally. Sure, no problem:
//!
//!```
//!use rustful::handler::{Variables, TreeRouter};
//!
//!let my_router = TreeRouter::<Option<Variables<ExampleHandler>>>::new();
//!# struct ExampleHandler;
//!# impl rustful::Handler for ExampleHandler {
//!#     fn handle(&self, _: rustful::Context, _: rustful::Response){}
//!# }
//!```
//!
//!And what about those route variables? Not using them at all? Well, just
//!remove them too, if you don't want them:
//!
//!```
//!use rustful::handler::TreeRouter;
//!
//!let my_router = TreeRouter::<Option<ExampleHandler>>::new();
//!# struct ExampleHandler;
//!# impl rustful::Handler for ExampleHandler {
//!#     fn handle(&self, _: rustful::Context, _: rustful::Response){}
//!# }
//!```
//!
//!You can simply recombine and reorder the router types however you want, or
//!why not make your own router?
//!
//![tree_router]: struct.TreeRouter.html
//![method_router]: struct.MethodRouter.html
//![variables]: struct.Variables.html
//![handle_request]: trait.HandleRequest.html
//![build]: trait.Build.html

use std::borrow::Cow;
use std::sync::Arc;
use anymap;

use context::{Context, MaybeUtf8Owned};
use context::hypermedia::Link;
use response::{Response, SendResponse};
use self::routing::RouteState;
use StatusCode;

pub use self::tree_router::TreeRouter;
pub use self::method_router::MethodRouter;
pub use self::variables::Variables;
pub use self::or_else::OrElse;
pub use self::status_router::StatusRouter;

pub mod routing;

pub mod tree_router;
pub mod method_router;
pub mod or_else;
pub mod status_router;
mod variables;

///Alias for `TreeRouter<MethodRouter<Variables<T>>>`.
///
///This is probably the most common composition, which will select handlers
///based on path and then HTTP method, and collect any variables on the way.
pub type DefaultRouter<T> = TreeRouter<MethodRouter<Variables<T>>>;

///A trait for request handlers.
pub trait Handler: Send + Sync + 'static {
    ///Handle a request from the client. Panicking within this method is
    ///discouraged, to allow the server to run smoothly.
    fn handle(&self, context: Context, response: Response);

    ///Get a description for the handler.
    fn description(&self) -> Option<Cow<'static, str>> {
        None
    }
}

impl<F: Fn(Context, Response) + Send + Sync + 'static> Handler for F {
    fn handle(&self, context: Context, response: Response) {
        self(context, response);
    }
}

impl<T: Handler> Handler for Arc<T> {
    fn handle(&self, context: Context, response: Response) {
        (**self).handle(context, response);
    }
}

impl Handler for Box<Handler> {
    fn handle(&self, context: Context, response: Response) {
        (**self).handle(context, response);
    }
}

///A request environment, containing the context, response and route state.
pub struct Environment<'a, 'b: 'a, 'l, 'g> {
    ///The request context, containing the request parameters.
    pub context: Context<'a, 'b, 'l, 'g>,

    ///The response that will be sent to the client.
    pub response: Response<'a, 'g>,

    ///A representation of the current routing state.
    pub route_state: RouteState<'g>,
}

impl<'a, 'b, 'l, 'g> Environment<'a, 'b, 'l, 'g> {
    ///Replace the hyperlinks. This consumes the whole request environment and
    ///returns a new one with a different lifetime, together with the old
    ///hyperlinks.
    pub fn replace_hyperlinks<'n>(self, hyperlinks: Vec<Link<'n>>) -> (Environment<'a, 'b, 'n, 'g>, Vec<Link<'l>>) {
        let (context, old_hyperlinks) = self.context.replace_hyperlinks(hyperlinks);
        (
            Environment {
                context: context,
                response: self.response,
                route_state: self.route_state
            },
            old_hyperlinks
        )
    }
}

///A low level request handler.
///
///Suitable for implementing routers, since it has complete access to the
///routing environment and can reject the request.
pub trait HandleRequest: Send + Sync + 'static {
    ///Try to handle an incoming request from the client, or return the
    ///request environment to the parent handler.
    fn handle_request<'a, 'b, 'l, 'g>(&self, environment: Environment<'a, 'b, 'l, 'g>) -> Result<(), Environment<'a, 'b, 'l, 'g>>;

    ///List all of the hyperlinks into this handler, based on the provided
    ///base link. It's up to the handler implementation to decide how deep to
    ///go.
    fn hyperlinks<'a>(&'a self, base_link: Link<'a>) -> Vec<Link<'a>>;
}

impl<H: Handler> HandleRequest for H {
    fn handle_request<'a, 'b, 'l, 'g>(&self, environment: Environment<'a, 'b, 'l, 'g>) -> Result<(), Environment<'a, 'b, 'l, 'g>> {
        self.handle(environment.context, environment.response);
        Ok(())
    }

    fn hyperlinks<'a>(&'a self, mut base_link: Link<'a>) -> Vec<Link<'a>> {
        base_link.handler = Some(self);
        vec![base_link]
    }
}

impl<H: HandleRequest> HandleRequest for Option<H> {
    fn handle_request<'a, 'b, 'l, 'g>(&self, mut environment: Environment<'a, 'b, 'l, 'g>) -> Result<(), Environment<'a, 'b, 'l, 'g>> {
        if let Some(ref handler) = *self {
            handler.handle_request(environment)
        } else {
            environment.response.set_status(StatusCode::NotFound);
            Err(environment)
        }
    }

    fn hyperlinks<'a>(&'a self, base_link: Link<'a>) -> Vec<Link<'a>> {
        if let Some(ref handler) = *self {
            handler.hyperlinks(base_link)
        } else {
            vec![]
        }
    }
}

///An adapter for simple content creation handlers.
///
///```
///use rustful::{DefaultRouter, Context, Server, ContentFactory};
///use rustful::Method::Get;
///
///type Router<T> = DefaultRouter<ContentFactory<T>>;
///
///let mut router = Router::new();
///router.build().path("*").then().on_get(|context: Context| format!("Visiting {}", context.uri_path));
///
///let server = Server {
///    handlers: router,
///    ..Server::default()
///};
///```
pub struct ContentFactory<T: CreateContent>(pub T);

impl<T: CreateContent, H: Into<ContentFactory<T>>> FromHandler<H> for ContentFactory<T> {
    fn from_handler(_context: BuilderContext, handler: H) -> ContentFactory<T> {
        handler.into()
    }
}

impl<T: CreateContent> ApplyContext for ContentFactory<T> {
    fn apply_context(&mut self, _context: BuilderContext) {}
    fn prepend_context(&mut self, _context: BuilderContext) {}
}

impl<T: CreateContent> Merge for ContentFactory<T> {
    fn merge(&mut self, other: ContentFactory<T>) {
        *self = other;
    }
}

impl<T: CreateContent> HandleRequest for ContentFactory<T> {
    fn handle_request<'a, 'b, 'l, 'g>(&self, environment: Environment<'a, 'b, 'l, 'g>) -> Result<(), Environment<'a, 'b, 'l, 'g>> {
        environment.response.send(self.0.create_content(environment.context));
        Ok(())
    }

    fn hyperlinks<'a>(&'a self, base_link: Link<'a>) -> Vec<Link<'a>> {
        vec![base_link]
    }
}

impl<T: CreateContent> From<T> for ContentFactory<T> {
    fn from(factory: T) -> ContentFactory<T> {
        ContentFactory(factory)
    }
}

///A simplified handler that returns the response content.
///
///It's meant for situations where direct access to the `Response` is
///unnecessary. The benefit is that it can use early returns in a more
///convenient way.
pub trait CreateContent: Send + Sync + 'static {
    ///The type of content that is created.
    type Output: for<'a, 'b> SendResponse<'a, 'b>;

    ///Create content that will be sent to the client.
    fn create_content(&self, context: Context) -> Self::Output;
}

impl<T, R> CreateContent for T where
    T: Fn(Context) -> R + Send + Sync + 'static,
    R: for<'a, 'b> SendResponse<'a, 'b>
{
    type Output = R;

    fn create_content(&self, context: Context) -> Self::Output {
        self(context)
    }
}

/// A collection of information that can be passed to child handlers while
/// building a handler.
pub type BuilderContext = anymap::Map<anymap::any::CloneAny>;

/// A trait for handlers that can be build, using a chainable API.
pub trait Build<'a> {
    /// The type that provides the builder API.
    type Builder: 'a;

    /// Get the builder type for this type, and prepare it with a context.
    fn get_builder(&'a mut self, context: BuilderContext) -> Self::Builder;
}

/// Create a handler from another handler.
pub trait FromHandler<T> {
    /// Create a handler from another handler and a `BuilderContext`.
    fn from_handler(context: BuilderContext, handler: T) -> Self;
}

impl<T: Handler> FromHandler<T> for T {
    fn from_handler(_context: BuilderContext, handler: T) -> T {
        handler
    }
}

impl<T: FromHandler<H>, H> FromHandler<H> for Option<T> {
    fn from_handler(context: BuilderContext, handler: H) -> Option<T> {
        Some(T::from_handler(context, handler))
    }
}

/// Apply a `BuilderContext` in various ways.
pub trait ApplyContext {
    /// Set properties, based on a given context.
    fn apply_context(&mut self, context: BuilderContext);

    /// Prepend existing properties, based on a given context.
    fn prepend_context(&mut self, context: BuilderContext);
}

impl<T: Handler> ApplyContext for T {
    fn apply_context(&mut self, _context: BuilderContext) {}
    fn prepend_context(&mut self, _context: BuilderContext) {}
}

impl<T: ApplyContext> ApplyContext for Option<T> {
    fn apply_context(&mut self, context: BuilderContext) {
        self.as_mut().map(|handler| handler.apply_context(context));
    }
    fn prepend_context(&mut self, context: BuilderContext) {
        self.as_mut().map(|handler| handler.prepend_context(context));
    }
}

/// A trait for handlers that can be merged.
pub trait Merge {
    /// Combine this handler with another, overwriting conflicting properties.
    fn merge(&mut self, other: Self);
}

impl<T: Handler> Merge for T {
    fn merge(&mut self, other: T) {
        *self = other;
    }
}

impl<T: Merge> Merge for Option<T> {
    fn merge(&mut self, other: Option<T>) {
        if let &mut Some(ref mut this_handler) = self {
            if let Some(other_handler) = other {
                this_handler.merge(other_handler);
            }
        } else {
            *self = other;
        }
    }
}

///Context type for storing path variable names.
#[derive(Clone, Debug, Default)]
pub struct VariableNames(pub Vec<MaybeUtf8Owned>);
