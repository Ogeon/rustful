//!Request and context plugins.

#![stable]

use StatusCode;
use header::Headers;

use context::Context;
use log::Log;

use response::{ResponseData, IntoResponseData};

///A trait for context plugins.
///
///They are able to modify and react to a `Context` before it's sent to the handler.
#[unstable = "plugin methods and parameters will change when a shared context is added"]
pub trait ContextPlugin {
    type Cache;

    ///Try to modify the `Context`.
    #[unstable = "plugin methods and parameters will change when a shared context is added"]
    fn modify(&self, log: &Log, context: &mut Context<Self::Cache>) -> ContextAction;
}

///The result from a context plugin.
#[unstable = "plugin methods and parameters will change when a shared context is added"]
#[derive(Copy)]
pub enum ContextAction {
    ///Continue to the next plugin in the stack.
    #[unstable = "plugin methods and parameters will change when a shared context is added"]
    Continue,

    ///Abort and send HTTP status.
    #[unstable = "plugin methods and parameters will change when a shared context is added"]
    Abort(StatusCode)
}


///A trait for response plugins.
///
///They are able to modify headers and data before it gets written in the response.
#[unstable = "plugin methods and parameters will change when a shared context is added"]
pub trait ResponsePlugin {
    ///Set or modify headers before they are sent to the client and maybe initiate the body.
    #[unstable = "plugin methods and parameters will change when a shared context is added"]
    fn begin(&self, log: &Log, status: StatusCode, headers: Headers) ->
        (StatusCode, Headers, ResponseAction);

    ///Handle content before writing it to the body.
    #[unstable = "plugin methods and parameters will change when a shared context is added"]
    fn write<'a>(&'a self, log: &Log, content: Option<ResponseData<'a>>) -> ResponseAction;

    ///End of body writing. Last chance to add content.
    #[unstable = "plugin methods and parameters will change when a shared context is added"]
    fn end(&self, log: &Log) -> ResponseAction;
}

///The result from a `ResponsePlugin`.
#[unstable = "plugin methods and parameters will change when a shared context is added"]
pub enum ResponseAction<'a> {
    ///Continue to the next plugin and maybe write data.
    #[unstable = "plugin methods and parameters will change when a shared context is added"]
    Write(Option<ResponseData<'a>>),

    ///Do not continue to the next plugin.
    #[unstable = "plugin methods and parameters will change when a shared context is added"]
    DoNothing,

    ///Abort with an error.
    #[unstable = "plugin methods and parameters will change when a shared context is added"]
    Error(String)
}

#[unstable = "plugin methods and parameters will change when a shared context is added"]
impl<'a> ResponseAction<'a> {
    #[unstable = "plugin methods and parameters will change when a shared context is added"]
    pub fn write<T: IntoResponseData<'a>>(data: Option<T>) -> ResponseAction<'a> {
        ResponseAction::Write(data.map(|d| d.into_response_data()))
    }

    #[unstable = "plugin methods and parameters will change when a shared context is added"]
    pub fn do_nothing() -> ResponseAction<'static> {
        ResponseAction::DoNothing
    }

    #[unstable = "plugin methods and parameters will change when a shared context is added"]
    pub fn error(message: String) -> ResponseAction<'static> {
        ResponseAction::Error(message)
    }
}