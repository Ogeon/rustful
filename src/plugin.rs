use StatusCode;
use header::Headers;

use context::Context;

use response::{ResponseData, IntoResponseData};

///A trait for context plugins.
///
///They are able to modify and react to a `Context` before it's sent to the handler.
#[experimental]
pub trait ContextPlugin {
    type Cache;

    ///Try to modify the `Context`.
    fn modify(&self, context: &mut Context<Self::Cache>) -> ContextAction;
}

///The result from a context plugin.
#[experimental]
#[derive(Copy)]
pub enum ContextAction {
    ///Continue to the next plugin in the stack.
    Continue,

    ///Abort and send HTTP status.
    Abort(StatusCode)
}


///A trait for response plugins.
///
///They are able to modify headers and data before it gets written in the response.
#[experimental]
pub trait ResponsePlugin {
    ///Set or modify headers before they are sent to the client and maybe initiate the body.
    fn begin(&self, status: StatusCode, headers: Headers) ->
        (StatusCode, Headers, ResponseAction);

    ///Handle content before writing it to the body.
    fn write<'a>(&'a self, content: Option<ResponseData<'a>>) -> ResponseAction;

    ///End of body writing. Last chance to add content.
    fn end(&self) -> ResponseAction;
}

///The result from a `ResponsePlugin`.
#[experimental]
pub enum ResponseAction<'a> {
    ///Continue to the next plugin and maybe write data.
    Write(Option<ResponseData<'a>>),

    ///Do not continue to the next plugin.
    DoNothing,

    ///Abort with an error.
    Error(String)
}

impl<'a> ResponseAction<'a> {
    pub fn write<T: IntoResponseData<'a>>(data: Option<T>) -> ResponseAction<'a> {
        ResponseAction::Write(data.map(|d| d.into_response_data()))
    }

    pub fn do_nothing() -> ResponseAction<'static> {
        ResponseAction::DoNothing
    }

    pub fn error(message: String) -> ResponseAction<'static> {
        ResponseAction::Error(message)
    }
}