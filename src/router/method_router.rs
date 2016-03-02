use std::collections::hash_map::{HashMap, Entry};

use router::{Router, Endpoint, InsertState, RouteState};
use context::hypermedia::Link;
use Method;

///A router which selects a handler from an HTTP method.
#[derive(Clone)]
pub struct MethodRouter<T> {
    items: HashMap<Method, T>,
}

impl<T: Router> Router for MethodRouter<T> {
    type Handler = T::Handler;

    fn find<'a>(&'a self, method: &Method, route: &mut RouteState) -> Endpoint<'a, Self::Handler> {
        if let Some(item) = self.items.get(method) {
            item.find(method, route)
        } else {
            Endpoint::from(None)
        }
    }

    fn hyperlinks<'a>(&'a self, base: Link<'a>) -> Vec<Link<'a>> {
        self.items.iter().flat_map(|(method, item)| {
            let mut link = base.clone();
            link.method = Some(method.clone());
            item.hyperlinks(link)
        }).collect()
    }

    fn build<'a, R: Into<InsertState<'a, I>>, I: Iterator<Item = &'a [u8]>>(method: Method, route: R, item: Self::Handler) -> MethodRouter<T> {
        let mut router = MethodRouter::default();
        router.insert(method, route, item);
        router
    }

    fn insert<'a, R: Into<InsertState<'a, I>>, I: Iterator<Item = &'a [u8]>>(&mut self, method: Method, route: R, item: Self::Handler) {
        self.items.insert(method.clone(), T::build(method, route, item));
    }

    fn insert_router<'a, R: Into<InsertState<'a, I>>, I: Clone + Iterator<Item = &'a [u8]>>(&mut self, route: R, router: MethodRouter<T>) {
        let route = route.into();
        for (method, mut item) in router.items {
            match self.items.entry(method) {
                Entry::Occupied(mut e) => {
                    e.get_mut().insert_router(route.clone(), item);
                },
                Entry::Vacant(e) => {
                    item.prefix(route.clone());
                    e.insert(item);
                }
            }
        }
    }

    fn prefix<'a, R: Into<InsertState<'a, I>>, I: Clone + Iterator<Item = &'a [u8]>>(&mut self, route: R) {
        let route = route.into();

        for (_, item) in &mut self.items {
            item.prefix(route.clone());
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
