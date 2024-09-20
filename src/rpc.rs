use rhai::FuncArgs;
use serde_json::value::RawValue;

pub struct RpcRequester {
    pub req_tx: crossbeam_channel::Sender<Vec<u8>>,
    pub res_rx: crossbeam_channel::Receiver<Vec<u8>>,
}

pub struct RpcResponder {
    pub req_rx: crossbeam_channel::Receiver<Vec<u8>>,
    pub res_tx: crossbeam_channel::Sender<Vec<u8>>,
}

pub fn make_channel(cap: usize) -> (RpcRequester, RpcResponder) {
    let (req_tx, req_rx) = crossbeam_channel::bounded(cap);
    let (res_tx, res_rx) = crossbeam_channel::bounded(cap);

    (
        RpcRequester { req_tx, res_rx },
        RpcResponder { req_rx, res_tx },
    )
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
    pub error: rhai::Dynamic,
}

pub struct JsonFuncArgs<'a>(pub &'a RawValue);

impl<'a> FuncArgs for JsonFuncArgs<'a> {
    fn parse<ARGS: Extend<rhai::Dynamic>>(self, args: &mut ARGS) {
        let parsed: Vec<rhai::Dynamic> = serde_json::from_str(self.0.get()).unwrap();
        args.extend(parsed);
    }
}
