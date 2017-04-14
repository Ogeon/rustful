use context::MaybeUtf8Owned;
use context::hypermedia::Link;
use handler::{HandleRequest, Environment, FromHandler, Build, BuilderContext, ApplyContext, Merge, VariableNames};

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

impl<T: FromHandler<H>, H> FromHandler<H> for Variables<T> {
    fn from_handler(mut context: BuilderContext, handler: H) -> Variables<T> {
        Variables {
            variables: context.remove::<VariableNames>().unwrap_or_default().0,
            handler: T::from_handler(context, handler),
        }
    }
}

impl<T: ApplyContext> ApplyContext for Variables<T> {
    fn apply_context(&mut self, mut context: BuilderContext) {
        if let Some(VariableNames(variables)) = context.remove() {
            self.variables = variables;
        }

        self.handler.apply_context(context);
    }

    fn prepend_context(&mut self, mut context: BuilderContext) {
        if let Some(VariableNames(mut variables)) = context.remove() {
            ::std::mem::swap(&mut self.variables, &mut variables);
            self.variables.extend(variables);
        }

        self.handler.prepend_context(context);
    }
}

impl<T: Merge> Merge for Variables<T> {
    fn merge(&mut self, other: Variables<T>) {
        self.variables = other.variables;
        self.handler.merge(other.handler);
    }
}

impl<'a, H: Build<'a>> Build<'a> for Variables<H> {
    type Builder = H::Builder;

    fn get_builder(&'a mut self, mut context: BuilderContext) -> Self::Builder {
        context.remove::<VariableNames>();
        self.handler.get_builder(context)
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

impl<H: Default> Default for Variables<H> {
    fn default() -> Variables<H> {
        Variables {
            handler: H::default(),
            variables: vec![],
        }
    }
}
