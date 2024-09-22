use rhai::FuncArgs;
use serde_json::value::RawValue;
use thingbuf::mpsc::{
    self,
    blocking::{Receiver, Sender, StaticChannel, StaticSender},
};

#[repr(usize)]
pub enum MessageSource {
    BleRpc,
    BleLovense,
    HttpRpc,
    Timer,
    // WsRpc,
    // Invalid
}

pub struct RequestMessage {
    pub buffer: Vec<u8>,
    pub src: MessageSource,
}

pub struct ResponseMessage {
    pub buffer: Vec<u8>,
    pub tag: ResponseTag,
}

#[repr(u8)]
pub enum ResponseTag {
    Normal,
    Lovense,
    BleRpc,
    Log,
}

pub struct MessageRecycler {
    pub min_size: usize,
    pub max_size: usize,
}

// const DEFAULT_MESSAGE_CAP: usize = 128;

impl MessageRecycler {
    pub const fn new(min_size: usize, max_size: usize) -> Self {
        MessageRecycler { min_size, max_size }
    }
}

impl thingbuf::Recycle<RequestMessage> for MessageRecycler {
    fn new_element(&self) -> RequestMessage {
        RequestMessage {
            buffer: Vec::with_capacity(self.min_size),
            src: MessageSource::BleRpc,
        }
    }

    fn recycle(&self, element: &mut RequestMessage) {
        element.buffer.clear();
        element.buffer.shrink_to(self.max_size);
        element.src = MessageSource::BleRpc;
    }
}

impl thingbuf::Recycle<ResponseMessage> for MessageRecycler {
    fn new_element(&self) -> ResponseMessage {
        ResponseMessage {
            buffer: Vec::with_capacity(self.min_size),
            tag: ResponseTag::Normal,
        }
    }

    fn recycle(&self, element: &mut ResponseMessage) {
        element.buffer.clear();
        element.buffer.shrink_to(self.max_size);
        element.tag = ResponseTag::Normal;
    }
}

pub static REQUEST_QUEUE: StaticChannel<RequestMessage, 16, MessageRecycler> =
    StaticChannel::<RequestMessage, 16, MessageRecycler>::with_recycle(MessageRecycler::new(
        32, 512,
    ));

pub struct RpcRequester {
    pub req_tx: StaticSender<RequestMessage, MessageRecycler>,
    pub res_rx: Receiver<ResponseMessage, MessageRecycler>,
}

pub type RpcResponder = Sender<ResponseMessage, MessageRecycler>;

pub struct ChannelOptions {
    pub message_capacity: usize,
    pub min_buffer_size: usize,
    pub max_buffer_size: usize,
}

pub fn make_channel(
    req_tx: StaticSender<RequestMessage, MessageRecycler>,
    opts: ChannelOptions,
) -> (RpcRequester, RpcResponder) {
    let (res_tx, res_rx) = mpsc::blocking::with_recycle(
        opts.message_capacity,
        MessageRecycler::new(opts.min_buffer_size, opts.max_buffer_size),
    );

    (RpcRequester { req_tx, res_rx }, res_tx)
}

#[derive(serde::Deserialize)]
pub struct RpcCall<'a> {
    pub method: &'a str,
    pub id: u8,
    pub params: &'a RawValue,
}

#[derive(serde::Serialize)]
pub struct RpcResponse {
    pub id: u8,
    pub result: rhai::Dynamic,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub struct JsonFuncArgs<'a>(pub &'a RawValue);

impl<'a> FuncArgs for JsonFuncArgs<'a> {
    fn parse<ARGS: Extend<rhai::Dynamic>>(self, args: &mut ARGS) {
        let parsed: Vec<rhai::Dynamic> = serde_json::from_str(self.0.get()).unwrap();
        args.extend(parsed);
    }
}
