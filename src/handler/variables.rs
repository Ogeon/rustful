use context::MaybeUtf8Owned;
use context::hypermedia::Link;
use {Method, Handler};
use handler::{HandleRequest, Environment};
use handler::routing::{Insert, InsertExt, InsertState};

///Assigns names to route variables.
///
///It's technically a single-item router, with the purpose of pairing route
///variable names with input values, but it has to come after other routers,
///since it has be at the end of a router chain to know what variables to use.
///It won't do any other routing work, so make sure to pair it with, at least,
///a path based router.
#[derive(Clone)]
pub struct Variables<H> {
    handler: H,
    variables: Vec<MaybeUtf8Owned>,
}

impl<T: Insert<H>, H> Insert<H> for Variables<T> {
    fn build<'a, R: Into<InsertState<'a, I>>, I: Iterator<Item = &'a [u8]>>(method: Method, route: R, item: H) -> Variables<T> {
        let mut route = route.into();
        Variables {
            variables: route.variables(),
            handler: T::build(method, route, item),
        }
    }

    fn insert<'a, R: Into<InsertState<'a, I>>, I: Iterator<Item = &'a [u8]>>(&mut self, method: Method, route: R, item: H) {
        let mut route = route.into();
        self.variables = route.variables();
        self.handler.insert(method, route, item);
    }
}

impl<H: InsertExt> InsertExt for Variables<H> {
    fn insert_router<'a, R: Into<InsertState<'a, I>>, I: Clone + Iterator<Item = &'a [u8]>>(&mut self, route: R, router: Variables<H>) {
        let mut route = route.into();
        let mut new_vars = route.variables();
        new_vars.extend(self.variables.drain(..));
        self.variables = new_vars;
        self.handler.insert_router(route, router.handler);
    }

    fn prefix<'a, R: Into<InsertState<'a, I>>, I: Clone + Iterator<Item = &'a [u8]>>(&mut self, route: R) {
        let mut route = route.into();
        let mut new_vars = route.variables();
        new_vars.extend(self.variables.drain(..));
        self.variables = new_vars;
        self.handler.prefix(route);
    }
}

impl<H: HandleRequest> HandleRequest for Variables<H> {
    fn handle_request<'a, 'b, 'l, 'g>(&self, mut environment: Environment<'a, 'b, 'l, 'g>) -> Result<(), Environment<'a, 'b, 'l, 'g>> {
        environment.context.variables = environment.route_state.variables(&self.variables).into();
        self.handler.handle_request(environment)
    }

    fn hyperlinks<'a>(&'a self, base: Link<'a>) -> Vec<Link<'a>> {
        self.handler.hyperlinks(base)
    }
}

impl<H: Handler + Default> Default for Variables<H> {
    fn default() -> Variables<H> {
        Variables {
            handler: H::default(),
            variables: vec![],
        }
    }
}
