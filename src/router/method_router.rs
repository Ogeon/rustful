use std::collections::hash_map::{HashMap, Entry};

use router::{Insert, InsertState};
use context::hypermedia::Link;
use Method;
use handler::{HandleRequest, Environment};

///A router that selects an item from an HTTP method.
///
///It's a simple mapping between `Method` and a router `T`, while the
///requested path is ignored. It's therefore a good idea to pair a
///`MethodRouter` with an exhaustive path router of some sort.
#[derive(Clone)]
pub struct MethodRouter<T> {
    items: HashMap<Method, T>,
}

impl<T: Insert> Insert for MethodRouter<T> {
    type Handler = T::Handler;

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

impl<T: HandleRequest> HandleRequest for MethodRouter<T> {
    fn handle_request<'a, 'b, 'l, 'g>(&self, environment: Environment<'a, 'b, 'l, 'g>) -> Result<(), Environment<'a, 'b, 'l, 'g>> {
        if let Some(item) = self.items.get(&environment.context.method) {
            item.handle_request(environment)
        } else {
            Err(environment)
        }
    }

    fn hyperlinks<'a>(&'a self, base: Link<'a>) -> Vec<Link<'a>> {
        self.items.iter().flat_map(|(method, item)| {
            let mut link = base.clone();
            link.method = Some(method.clone());
            item.hyperlinks(link)
        }).collect()
    }
}

impl<T> Default for MethodRouter<T> {
    fn default() -> MethodRouter<T> {
        MethodRouter {
            items: HashMap::new(),
        }
    }
}
