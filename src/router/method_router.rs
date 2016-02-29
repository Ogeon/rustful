use std::collections::HashMap;

use router::{Router, Endpoint, InsertState, RouteState};
use context::MaybeUtf8Owned;
use context::hypermedia::Link;
use {Method, Handler};

///A router which selects a handler from an HTTP method.
pub struct MethodRouter<T> {
    items: HashMap<Method, (T, Vec<MaybeUtf8Owned>)>,
}

impl<T: Handler> Router for MethodRouter<T> {
    type Handler = T;

    fn find<'a>(&'a self, method: &Method, route: &mut RouteState) -> Endpoint<'a, T> {
        if let Some(&(ref h, ref variables)) = self.items.get(method) {
            Endpoint {
                handler: Some(h),
                variables: route.variables(variables),
                hyperlinks: vec![],
            }
        } else {
            Endpoint::from(None)
        }
    }

    fn hyperlinks<'a>(&'a self, base: Link<'a>) -> Vec<Link<'a>> {
        self.items.iter().map(|(method, &(ref item, _))| {
            let mut link = base.clone();
            link.method = Some(method.clone());
            link.handler = Some(item);
            link
        }).collect()
    }

    fn insert<'a, R: Into<InsertState<'a, I>>, I: Iterator<Item = &'a [u8]>>(&mut self, method: Method, route: R, item: Self::Handler) {
        self.items.insert(method, (item, route.into().variables()));
    }

    fn insert_router<'a, R: Into<InsertState<'a, I>>, I: Clone + Iterator<Item = &'a [u8]>>(&mut self, route: R, router: MethodRouter<T>) {
        let prefix = route.into().variables();
        for (method, (item, variables)) in router.items {
            let mut new_vars = prefix.clone();
            new_vars.extend(variables);
            self.items.insert(method, (item, new_vars));
        }
    }
}

impl<T> Default for MethodRouter<T> {
    fn default() -> MethodRouter<T> {
        MethodRouter {
            items: HashMap::new(),
        }
    }
}
