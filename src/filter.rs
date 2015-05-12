//!Request and context filters.

use std::any::Any;

use anymap::AnyMap;

use StatusCode;
use header::Headers;

use context::Context;
use log::Log;

use response::Data;

///Contextual tools for filters.
pub struct FilterContext<'a> {
    ///Shared storage for filters. It is local to the current request and
    ///accessible from the handler and all of the filters. It can be used to
    ///send data between these units.
    pub storage: &'a mut AnyMap,

    ///Log for notes, errors and warnings.
    pub log: &'a Log,

    ///Globally accessible data.
    pub global: &'a Any,
}

///A trait for context filters.
///
///They are able to modify and react to a `Context` before it's sent to the handler.
pub trait ContextFilter {
    ///Try to modify the handler `Context`.
    fn modify(&self, context: FilterContext, request_context: &mut Context) -> ContextAction;
}

///The result from a context filter.
pub enum ContextAction {
    ///Continue to the next filter in the stack.
    Next,

    ///Abort and send HTTP status.
    Abort(StatusCode)
}

impl<'a> ContextAction {
    pub fn next() -> ContextAction {
        ContextAction::Next
    }

    pub fn abort(status: StatusCode) -> ContextAction {
        ContextAction::Abort(status)
    }
}


///A trait for response filters.
///
///They are able to modify headers and data before it gets written in the response.
pub trait ResponseFilter {
    ///Set or modify headers before they are sent to the client and maybe initiate the body.
    fn begin(&self, context: FilterContext, status: StatusCode, headers: Headers) ->
        (StatusCode, Headers, ResponseAction);

    ///Handle content before writing it to the body.
    fn write<'a>(&'a self, context: FilterContext, content: Option<Data<'a>>) -> ResponseAction;

    ///End of body writing. Last chance to add content.
    fn end(&self, context: FilterContext) -> ResponseAction;
}

///The result from a response filter.
pub enum ResponseAction<'a> {
    ///Continue to the next filter and maybe write data.
    Next(Option<Data<'a>>),

    ///Do not continue to the next filter.
    SilentAbort,

    ///Abort with an error.
    Abort(String)
}

impl<'a> ResponseAction<'a> {
    pub fn next<T: Into<Data<'a>>>(data: Option<T>) -> ResponseAction<'a> {
        ResponseAction::Next(data.map(|d| d.into()))
    }

    pub fn silent_abort() -> ResponseAction<'a> {
        ResponseAction::SilentAbort
    }

    pub fn abort(message: String) -> ResponseAction<'a> {
        ResponseAction::Abort(message)
    }
}