use esp32_nimble::{
    enums::{AuthReq, SecurityIOCap},
    utilities::BleUuid,
    uuid128, BLEAdvertisementData, BLEDevice, NimbleProperties,
};

use crate::rpc::RpcRequester;

const RPC_REQ_CHAR: BleUuid = uuid128!("813f9733-95c9-49ba-84a0-d0167c260eef");
const RPC_RES_CHAR: BleUuid = uuid128!("23ad909d-511b-4fad-ad85-0bf102eee315");

const LOVENSE_RX_CHAR: BleUuid = uuid128!("54300002-0023-4bd4-bbd5-a6920e4c5653");
const LOVENSE_TX_CHAR: BleUuid = uuid128!("54300003-0023-4bd4-bbd5-a6920e4c5653");

const LOVENSE_SERVICE_ID: BleUuid = uuid128!("54300001-0023-4bd4-bbd5-a6920e4c5653");
// const ESPWAND_SERVICE_ID: BleUuid = uuid128!("af12176f-36e8-4d06-8a03-a1563f0a7baf");

// pub fn run_ble(req_tx: StaticSender<Vec<u8>>, res_rx: StaticReceiver<Vec<u8>>) {
pub fn run_ble(engine: RpcRequester, lovense: RpcRequester) {
    let device = BLEDevice::take();

    device
        .security()
        .set_auth(AuthReq::Bond) // Bonding enables key storage for reconnection
        .set_passkey(123456) // Optional, sets the passkey for pairing
        .set_io_cap(SecurityIOCap::NoInputNoOutput) // You can choose any IO capability
        .resolve_rpa(); // Crucial for managing iOS's dynamic Bluetooth addresses

    let advertising = device.get_advertising();

    let server = device.get_server();

    server.on_connect(|_, desc| {
        log::info!("hewwo to {desc:?}");
    });

    server.on_disconnect(|desc, reason| {
        log::info!("{desc:?} has left: {reason:?}");
    });

    server.on_authentication_complete(|desc, result| {
        log::info!("auth completed: {desc:?}: {result:?}")
    });

    let lovense_service = server.create_service(LOVENSE_SERVICE_ID);

    // let rpc_service = server.create_service(ESPWAND_SERVICE_ID);

    let request_char = lovense_service.lock().create_characteristic(
        RPC_REQ_CHAR,
        NimbleProperties::WRITE | NimbleProperties::WRITE_NO_RSP,
    );

    let response_char = lovense_service.lock().create_characteristic(
        RPC_RES_CHAR,
        NimbleProperties::READ | NimbleProperties::NOTIFY,
    );

    request_char.lock().on_write(move |args| {
        let _ = engine.req_tx.send(args.recv_data().to_vec());
    });

    let lovense_rx = lovense_service.lock().create_characteristic(
        LOVENSE_RX_CHAR,
        NimbleProperties::WRITE | NimbleProperties::WRITE_NO_RSP,
    );

    let lovense_tx = lovense_service.lock().create_characteristic(
        LOVENSE_TX_CHAR,
        NimbleProperties::READ | NimbleProperties::NOTIFY,
    );

    lovense_rx.lock().on_write(move |args| {
        let _ = lovense.req_tx.send(args.recv_data().to_vec());
        // println!("from lovense: {}", std::str::from_utf8(args.recv_data()).unwrap());
    });

    advertising
        .lock()
        .set_data(
            BLEAdvertisementData::new()
                .name("LOVE-Calor")
                // .add_service_uuid(ESPWAND_SERVICE_ID)
                .add_service_uuid(LOVENSE_SERVICE_ID),
        )
        .unwrap();

    advertising.lock().start().unwrap();

    loop {
        crossbeam_channel::select! {
            recv(engine.res_rx) -> res => {
                response_char.lock().set_value(res.unwrap().as_ref()).notify();
            },
            recv(lovense.res_rx) -> res => {
                lovense_tx.lock().set_value(res.unwrap().as_ref()).notify();
            }
        }
    }
}
// }
