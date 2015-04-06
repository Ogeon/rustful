//!Request and context plugins.

#![stable]

use anymap::AnyMap;

use StatusCode;
use header::Headers;

use context::Context;
use log::Log;

use response::ResponseData;

///Contextual tools for plugins.
pub struct PluginContext<'a> {
    ///Shared storage for plugins. It is local to the current request and
    ///accessible from the handler and all of the plugins. It can be used to
    ///send data between these units.
    pub storage: &'a mut AnyMap,

    ///Log for notes, errors and warnings.
    pub log: &'a Log
}

///A trait for context plugins.
///
///They are able to modify and react to a `Context` before it's sent to the handler.
pub trait ContextPlugin {
    ///Try to modify the handler `Context`.
    fn modify(&self, context: PluginContext, request_context: &mut Context) -> ContextAction;
}

///The result from a context plugin.
pub enum ContextAction {
    ///Continue to the next plugin in the stack.
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


///A trait for response plugins.
///
///They are able to modify headers and data before it gets written in the response.
pub trait ResponsePlugin {
    ///Set or modify headers before they are sent to the client and maybe initiate the body.
    fn begin(&self, context: PluginContext, status: StatusCode, headers: Headers) ->
        (StatusCode, Headers, ResponseAction);

    ///Handle content before writing it to the body.
    fn write<'a>(&'a self, context: PluginContext, content: Option<ResponseData<'a>>) -> ResponseAction;

    ///End of body writing. Last chance to add content.
    fn end(&self, context: PluginContext) -> ResponseAction;
}

///The result from a `ResponsePlugin`.
pub enum ResponseAction<'a> {
    ///Continue to the next plugin and maybe write data.
    Next(Option<ResponseData<'a>>),

    ///Do not continue to the next plugin.
    SilentAbort,

    ///Abort with an error.
    Abort(String)
}

impl<'a> ResponseAction<'a> {
    pub fn next<T: Into<ResponseData<'a>>>(data: Option<T>) -> ResponseAction<'a> {
        ResponseAction::Next(data.map(|d| d.into()))
    }

    pub fn silent_abort() -> ResponseAction<'a> {
        ResponseAction::SilentAbort
    }

    pub fn abort(message: String) -> ResponseAction<'a> {
        ResponseAction::Abort(message)
    }
}