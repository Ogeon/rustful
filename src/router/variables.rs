use router::{Router, Endpoint, InsertState, RouteState};
use context::MaybeUtf8Owned;
use context::hypermedia::Link;
use handler::Factory;
use Method;

///A router endpoint that assigns names to route variables.
///
///It's technically a single-item router, with the purpose of pairing route
///variable names with input values, but it can't contain other routers, since
///it has be at the end of a router chain to know what variables to use. It
///won't do any other routing work, so make sure to pair it with, at least, a
///path based router.
#[derive(Clone)]
pub struct Variables<H: Factory> {
    handler: H,
    variables: Vec<MaybeUtf8Owned>,
}

impl<H: Factory> Router for Variables<H> {
    type Handler = H;

    fn find<'a>(&'a self, _method: &Method, route: &mut RouteState) -> Endpoint<'a, H> {
        Endpoint {
            handler: Some(&self.handler),
            variables: route.variables(&self.variables),
            hyperlinks: vec![],
        }
    }

    fn hyperlinks<'a>(&'a self, mut base: Link<'a>) -> Vec<Link<'a>> {
        base.handler = Some(&self.handler);
        vec![base]
    }

    fn build<'a, R: Into<InsertState<'a, I>>, I: Iterator<Item = &'a [u8]>>(_method: Method, route: R, item: Self::Handler) -> Variables<H> {
        Variables {
            handler: item,
            variables: route.into().variables(),
        }
    }

    fn insert<'a, R: Into<InsertState<'a, I>>, I: Iterator<Item = &'a [u8]>>(&mut self, _method: Method, route: R, item: Self::Handler) {
        self.handler = item;
        self.variables = route.into().variables();
    }

    fn insert_router<'a, R: Into<InsertState<'a, I>>, I: Clone + Iterator<Item = &'a [u8]>>(&mut self, route: R, mut router: Variables<H>) {
        router.prefix(route);
        *self = router;
    }

    fn prefix<'a, R: Into<InsertState<'a, I>>, I: Clone + Iterator<Item = &'a [u8]>>(&mut self, route: R) {
        let mut new_vars = route.into().variables();
        new_vars.extend(self.variables.drain(..));
        self.variables = new_vars;
    }
}

impl<H: Factory + Default> Default for Variables<H> {
    fn default() -> Variables<H> {
        Variables {
            handler: H::default(),
            variables: vec![],
        }
    }
}
