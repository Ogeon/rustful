//! A sequence of content producing tasks.
use std::marker::PhantomData;
use {CreateContent, SendResponse, Context, Response};
use handler::{FromHandler, BuilderContext};

///! A sequence of content producing tasks.
pub struct Sequence<O> {
    tasks: Box<Tasks<Output=O>>
}

impl<O> Sequence<O> {
    /// Build a new `Sequence` from an initial task.
    pub fn build<F: Tasks>(task: F) -> Builder<Initial<F>, O> {
        Builder {
            task: Initial { task },
            phantom: PhantomData
        }
    }
}

impl<O: SendResponse + 'static> CreateContent for Sequence<O> {
    type Output = O;

    fn create_content(&self, context: &mut Context, response: &Response) -> Self::Output {
        self.tasks.perform(context, response)
    }
}

impl<O, T: Tasks<Output=O>> FromHandler<T> for Sequence<O> {
    fn from_handler(_: BuilderContext, handler: T) -> Sequence<O> {
        Sequence {
            tasks: Box::new(handler),
        }
    }
}

/// A builder for a `Sequence`.
pub struct Builder<T, U> {
    task: T,
    phantom: PhantomData<Sequence<U>>
}

impl<T: Tasks, U> Builder<T, U> {
    /// Append another task to the sequence.
    pub fn then<F: Task<T::Output>>(self, task: F) -> Builder<Step<T, F>, U> {
        Builder {
            task: Step {
                previous: self.task,
                next: task
            },
            phantom: PhantomData
        }
    }
}

impl<T: Tasks> Builder<T, T::Output> {
    /// Finish building the sequence.
    pub fn done(self) -> Sequence<T::Output> {
        Sequence {
            tasks: Box::new(self.task)
        }
    }
}

/// A task at the beginning of a sequence.
pub trait Tasks: Send + Sync + 'static {
    /// The output from the task.
    type Output;

    /// Perform the task.
    fn perform(&self, context: &mut Context, response: &Response) -> Self::Output;
}

impl<F, T> Tasks for F where
    F: Fn(&mut Context, &Response) -> T,
    F: Send + Sync + 'static
{
    type Output = T;

    #[inline(always)]
    fn perform(&self, context: &mut Context, response: &Response) -> T {
        self(context, response)
    }
}

impl<O: 'static> Tasks for Box<Tasks<Output=O>> {
    type Output = O;

    #[inline(always)]
    fn perform(&self, context: &mut Context, response: &Response) -> O {
        (**self).perform(context, response)
    }
}

/// A task in a sequence.
pub trait Task<I>: Send + Sync + 'static {
    /// The output from the task.
    type Output;

    /// Perform the task.
    fn perform(&self, context: &mut Context, response: &Response, input: I) -> Self::Output;
}

impl<F, I, T> Task<I> for F where
    F: Fn(&mut Context, &Response, I) -> T,
    F: Send + Sync + 'static
{
    type Output = T;

    #[inline(always)]
    fn perform(&self, context: &mut Context, response: &Response, input: I) -> T {
        self(context, response, input)
    }
}

/// The initial task in a sequence.
pub struct Initial<T> {
    task: T
}

impl<T: Tasks> Task<()> for Initial<T> {
    type Output = T::Output;

    #[inline(always)]
    fn perform(&self, context: &mut Context, response: &Response, _: ()) -> Self::Output {
        self.task.perform(context, response)
    }
}

impl<T: Tasks> Tasks for Initial<T> {
    type Output = T::Output;

    #[inline(always)]
    fn perform(&self, context: &mut Context, response: &Response) -> Self::Output {
        self.task.perform(context, response)
    }
}

/// A link from one task to another.
pub struct Step<T, U> {
    previous: T,
    next: U
}

impl<T: Task<I>, U: Task<T::Output>, I> Task<I> for Step<T, U> {
    type Output = U::Output;

    #[inline(always)]
    fn perform(&self, context: &mut Context, response: &Response, input: I) -> Self::Output {
        let next_input = self.previous.perform(context, response, input);
        self.next.perform(context, response, next_input)
    }
}

impl<T: Task<()>, U: Task<T::Output>> Tasks for Step<T, U> {
    type Output = U::Output;

    #[inline(always)]
    fn perform(&self, context: &mut Context, response: &Response) -> Self::Output {
        let next_input = self.previous.perform(context, response, ());
        self.next.perform(context, response, next_input)
    }
}
