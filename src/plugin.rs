//!Request and context plugins.

#![stable]

use anymap::AnyMap;

use StatusCode;
use header::Headers;

use context::Context;
use log::Log;

use response::{ResponseData, IntoResponseData};

///Contextual tools for plugins.
pub struct PluginContext<'a> {
    ///Shared storage for plugins. Local to the current request.
    pub storage: &'a mut AnyMap,

    ///Log for notes, errors and warnings.
    pub log: &'a Log
}

///A trait for context plugins.
///
///They are able to modify and react to a `Context` before it's sent to the handler.
#[unstable = "plugin context is not finalized"]
pub trait ContextPlugin {
    ///Try to modify the `Context`.
    #[unstable = "plugin context is not finalized"]
    fn modify(&self, context: PluginContext, request_context: &mut Context) -> ContextAction;
}

///The result from a context plugin.
#[unstable = "plugin context is not finalized"]
pub enum ContextAction {
    ///Continue to the next plugin in the stack.
    #[unstable = "plugin context is not finalized"]
    Continue,

    ///Abort and send HTTP status.
    #[unstable = "plugin context is not finalized"]
    Abort(StatusCode)
}


///A trait for response plugins.
///
///They are able to modify headers and data before it gets written in the response.
#[unstable = "plugin context is not finalized"]
pub trait ResponsePlugin {
    ///Set or modify headers before they are sent to the client and maybe initiate the body.
    #[unstable = "plugin context is not finalized"]
    fn begin(&self, context: PluginContext, status: StatusCode, headers: Headers) ->
        (StatusCode, Headers, ResponseAction);

    ///Handle content before writing it to the body.
    #[unstable = "plugin context is not finalized"]
    fn write<'a>(&'a self, context: PluginContext, content: Option<ResponseData<'a>>) -> ResponseAction;

    ///End of body writing. Last chance to add content.
    #[unstable = "plugin context is not finalized"]
    fn end(&self, context: PluginContext) -> ResponseAction;
}

///The result from a `ResponsePlugin`.
#[unstable = "plugin context is not finalized"]
pub enum ResponseAction<'a> {
    ///Continue to the next plugin and maybe write data.
    #[unstable = "plugin context is not finalized"]
    Write(Option<ResponseData<'a>>),

    ///Do not continue to the next plugin.
    #[unstable = "plugin context is not finalized"]
    DoNothing,

    ///Abort with an error.
    #[unstable = "plugin context is not finalized"]
    Error(String)
}

#[unstable = "plugin context is not finalized"]
impl<'a> ResponseAction<'a> {
    #[unstable = "plugin context is not finalized"]
    pub fn write<T: IntoResponseData<'a>>(data: Option<T>) -> ResponseAction<'a> {
        ResponseAction::Write(data.map(|d| d.into_response_data()))
    }

    #[unstable = "plugin context is not finalized"]
    pub fn do_nothing() -> ResponseAction<'static> {
        ResponseAction::DoNothing
    }

    #[unstable = "plugin context is not finalized"]
    pub fn error(message: String) -> ResponseAction<'static> {
        ResponseAction::Error(message)
    }
}