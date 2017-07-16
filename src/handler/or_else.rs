//! A router that selects a secondary handler on error.

use context::hypermedia::Link;
use handler::{HandleRequest, Environment, Build, BuilderContext, ApplyContext, Merge};

/// A router that selects a secondary handler on error.
///
/// ```
/// use rustful::{OrElse, Context, Response};
///
/// fn error_handler(_: &mut Context, response: &Response) -> String {
///     format!("Status: {}", response.status())
/// }
///
/// //Every request will end up at error_handler
/// let handler = OrElse::<Option<fn(&mut Context) -> String>, _>::new(None, error_handler);
/// ```
#[derive(Clone)]
pub struct OrElse<A, B> {
    ///The primary handler that will have the first chance to process the
    ///request.
    pub primary: A,

    ///The secondary handler that will take over if the first handler returns
    ///an error.
    pub secondary: B,
}

impl<A, B> OrElse<A, B> {
    ///Create a new `OrElse` with a primary and secondary handler.
    pub fn new(primary: A, secondary: B) -> OrElse<A, B> {
        OrElse {
            primary: primary,
            secondary: secondary,
        }
    }

    /// Build the router and its children using a chaninable API.
    ///
    /// ```
    /// use rustful::{Context, OrElse};
    /// use rustful::handler::MethodRouter;
    ///
    /// type Inner = MethodRouter<fn(&mut Context) -> &'static str>;
    ///
    /// fn get(_context: &mut Context) -> &'static str {
    ///     "A GET request."
    /// }
    ///
    /// fn post(_context: &mut Context) -> &'static str {
    ///     "A POST request."
    /// }
    ///
    /// fn or_else(_context: &mut Context) -> &'static str {
    ///     "Hello world!"
    /// }
    ///
    /// let mut router = OrElse::<Inner, _>::with_secondary(or_else);
    ///
    /// router.build().primary().many(|mut inner_router|{
    ///     inner_router.on_get(get);
    ///     inner_router.on_post(post);
    /// });
    /// ```
    pub fn build(&mut self) -> Builder<A, B> {
        self.get_builder(BuilderContext::new())
    }
}

impl<A, B: Default> OrElse<A, B> {
    ///Create a new `OrElse` with a given primary and a default secondary handler.
    pub fn with_primary(primary: A) -> OrElse<A, B> {
        OrElse {
            primary: primary,
            secondary: B::default(),
        }
    }
}

impl<A: Default, B> OrElse<A, B> {
    ///Create a new `OrElse` with a given secondary and a default primary handler.
    pub fn with_secondary(secondary: B) -> OrElse<A, B> {
        OrElse {
            primary: A::default(),
            secondary: secondary,
        }
    }
}

impl<A: HandleRequest, B: HandleRequest> HandleRequest for OrElse<A, B> {
    fn handle_request<'a, 'b, 'l, 'g>(&self, environment: Environment<'a, 'b, 'l, 'g>) -> Result<(), Environment<'a, 'b, 'l, 'g>> {
        self.primary.handle_request(environment).or_else(|e| self.secondary.handle_request(e))
    }

    fn hyperlinks<'a>(&'a self, base: Link<'a>) -> Vec<Link<'a>> {
        let mut links = self.primary.hyperlinks(base.clone());
        links.extend(self.secondary.hyperlinks(base.clone()));
        links
    }
}

impl<A: Default, B: Default> Default for OrElse<A, B> {
    fn default() -> OrElse<A, B> {
        OrElse {
            primary: A::default(),
            secondary: B::default(),
        }
    }
}

impl<'a, A: 'a, B: 'a> Build<'a> for OrElse<A, B> {
    type Builder = Builder<'a, A, B>;

    fn get_builder(&'a mut self, context: BuilderContext) -> Builder<'a, A, B> {
        Builder {
            router: self,
            context: context
        }
    }
}

impl<A: ApplyContext, B: ApplyContext> ApplyContext for OrElse<A, B> {
    fn apply_context(&mut self, context: BuilderContext) {
        self.primary.apply_context(context.clone());
        self.secondary.apply_context(context);
    }

    fn prepend_context(&mut self, context: BuilderContext) {
        self.primary.prepend_context(context.clone());
        self.secondary.prepend_context(context);
    }
}

impl<A: Merge, B: Merge> Merge for OrElse<A, B> {
    fn merge(&mut self, other: OrElse<A, B>) {
        self.primary.merge(other.primary);
        self.secondary.merge(other.secondary);
    }
}

/// A builder for an `OrElse`.
pub struct Builder<'a, A: 'a, B: 'a> {
    router: &'a mut OrElse<A, B>,
    context: BuilderContext
}

impl<'a, A, B> Builder<'a, A, B> {
    /// Perform more than one operation on this builder.
    ///
    /// ```
    /// use rustful::{Context, OrElse};
    /// use rustful::handler::MethodRouter;
    ///
    /// type Inner = MethodRouter<fn(&mut Context) -> &'static str>;
    ///
    /// fn get(_context: &mut Context) -> &'static str {
    ///     "A GET request."
    /// }
    ///
    /// fn post(_context: &mut Context) -> &'static str {
    ///     "A POST request."
    /// }
    ///
    /// let mut router = OrElse::<Inner, Inner>::default();
    ///
    /// router.build().many(|mut router|{
    ///     router.primary().on_get(get);
    ///     router.secondary().on_post(post);
    /// });
    /// ```
    pub fn many<F: FnOnce(&mut Builder<'a, A, B>)>(&mut self, build: F) -> &mut Builder<'a, A, B> {
        build(self);
        self
    }
}

impl<'a: 'b, 'b, A: Build<'b>, B> Builder<'a, A, B> {
    /// Build the primary router and its children.
    pub fn primary(&'b mut self) -> A::Builder {
        self.router.primary.get_builder(self.context.clone())
    }
}

impl<'a: 'b, 'b, A, B: Build<'b>> Builder<'a, A, B> {
    /// Build the secondary router and its children.
    pub fn secondary(&'b mut self) -> B::Builder {
        self.router.secondary.get_builder(self.context.clone())
    }
}
