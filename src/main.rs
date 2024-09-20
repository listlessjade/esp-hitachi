#![feature(maybe_uninit_slice)]

use std::{rc::Rc, str::FromStr, time::Instant};

use ble::run_ble;
use crossbeam_channel::select;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        ledc::{config::TimerConfig, LedcDriver, LedcTimerDriver},
        prelude::*,
    },
    mdns::EspMdns,
    nvs::{EspDefaultNvsPartition, EspNvs, EspNvsPartition, NvsCustom},
    sys::{esp, esp_mac_type_t_ESP_MAC_BT, esp_read_mac},
    timer::EspTaskTimerService,
    wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};
use http::run_http;
use log::info;
use parking_lot::Mutex;
use rhai_hal::{ledc::PwmController, timer::CallTimer};
use rpc::{JsonFuncArgs, RpcCall, RpcResponse};
use script::ScriptRunner;
// use script::ScriptRunner;
mod script;

const SSID: &str = "";
const PASSWORD: &str = "";

mod ble;
mod http;
mod rhai_hal;
mod rpc;

// static BLE_REQ_CHANNEL: StaticChannel<Vec<u8>, 16> = StaticChannel::new();
// static BLE_RES_CHANNEL: StaticChannel<Vec<u8>, 16> = StaticChannel::new();

// fn main() -> anyhow::Result<()> {
//     esp_idf_svc::sys::link_patches();
//     esp_idf_svc::log::EspLogger::initialize_default();

//     use esp_idf_svc::hal::ledc::{config::TimerConfig, LedcDriver, LedcTimerDriver};
//     use esp_idf_svc::hal::peripherals::Peripherals;
//     use esp_idf_svc::hal::prelude::*;

//     let peripherals = Peripherals::take().unwrap();
//     // let mut pin = PinDriver::output(peripherals.pins.gpio10).unwrap();
//     // pin.set_high().unwrap();

//     let max_duty = driver.get_max_duty();
//     driver.set_duty(max_duty * 3 / 4)?;

//     loop {
//         std::thread::sleep(Duration::from_secs(1));
//     }

//     Ok(())
// }

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Hello, world!");

    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let default_nvs = EspDefaultNvsPartition::take()?;

    let timer_driver = LedcTimerDriver::new(
        peripherals.ledc.timer0,
        &TimerConfig::default().frequency(25.kHz().into()),
    )
    .unwrap();
    let driver = LedcDriver::new(
        peripherals.ledc.channel0,
        timer_driver,
        peripherals.pins.gpio10,
    )?;

    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(default_nvs))?,
        sys_loop,
    )?;

    connect_wifi(&mut wifi)?;

    let mut mac = [0u8; 6];
    esp!(unsafe { esp_read_mac(mac.as_mut_ptr(), esp_mac_type_t_ESP_MAC_BT) })?;
    let mac_addr = format!(
        "{:<02X}{:<02X}{:<02X}{:<02X}{:<02X}{:<02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );

    let program_nvs_part = EspNvsPartition::<NvsCustom>::take("programs")?;
    let mut program_nvs = EspNvs::new(program_nvs_part, "scripts", true)?;

    // if let Some(program) = program_nvs.get_ra

    let mut mdns = EspMdns::take()?;

    mdns.set_hostname("esp-magicwand")?;
    mdns.set_instance_name("Magic Wand [v0.1]")?;
    mdns.add_service(None, "_magicwandrpc", "_tcp", 8080, &[("version", "0")])?;

    // std::thread::scope(move |s| {
    let mut script_engine = ScriptRunner::new();

    script_engine.insert_builtin(
        "pwm",
        PwmController {
            percent: 0,
            driver: Rc::new(Mutex::new(driver)),
        },
    );

    script_engine.insert_builtin("mac_address", mac_addr);

    let (timer_tx, timer_rx) = crossbeam_channel::bounded::<()>(0);
    let timer_service = EspTaskTimerService::new()?;

    script_engine.insert_builtin(
        "timer",
        CallTimer {
            interval: 0,
            inner: Rc::new(timer_service.timer(move || {
                timer_tx.send(()).unwrap();
            })?),
        },
    );

    if let Some(script_len) = program_nvs.blob_len("script")?.filter(|v| *v > 0) {
        let mut script_buf = vec![0; script_len];
        let script = program_nvs.get_blob("script", &mut script_buf)?.unwrap();
        let script = std::str::from_utf8(script).unwrap();
        let _ = dbg!(script_engine.recompile(script));
    }

    // let (ble_req_tx, ble_req_rx) = BLE_REQ_CHANNEL.split();
    // let (ble_res_tx, ble_res_rx) = BLE_RES_CHANNEL.split();
    let (ble_tx, ble_rx) = rpc::make_channel(4);
    let (http_tx, http_rx) = rpc::make_channel(4);
    let (lovense_tx, lovense_rx) = rpc::make_channel(4);
    // let (ws_tx, ws_rx) = rpc::make_channel(4);

    let _ble_thread = std::thread::spawn(|| run_ble(ble_tx, lovense_tx));
    let _http_server = run_http(http_tx, 8080);

    loop {
        let (req, res_channel) = select! {
            recv(ble_rx.req_rx) -> req => {
                (req.unwrap(), &ble_rx.res_tx)
            },
            recv(http_rx.req_rx) -> req => {
                (req.unwrap(), &http_rx.res_tx)
            },
            recv(timer_rx) -> _ => {
                let _ = script_engine.call::<rhai::Dynamic>("tick", (rhai::Dynamic::from_timestamp(Instant::now()), ));
                continue;
            },
            recv(lovense_rx.req_rx) -> req => {
                let req = String::from_utf8(req.unwrap()).unwrap();
                log::info!("lovense req: {req}");
                let end = req.find(';').unwrap();

                let args: Vec<rhai::Dynamic> = (req[..end].split_terminator(':')).map(|s| rhai::Dynamic::from_str(s).unwrap()).collect::<Vec<_>>();
                dbg!(&args);

                let res = script_engine.call::<rhai::Dynamic>("lovense", (args,));
                let res: Vec<String> = res.into_typed_array().unwrap();

                let mut res = res.join(":");
                res.push(';');
                log::info!("lovense ret: {res}");
                lovense_rx.res_tx.send(res.into_bytes()).unwrap();
                continue;
            }
            // recv(ws_rx.req_rx) -> req => {
            //     (req.unwrap(), &ws_rx.res_tx)
            // }
        };

        // let req = ble_req_rx.recv_ref().unwrap();
        // let req = ble_req_rx.recv().unwrap();

        let request: RpcCall<'_> = serde_json::from_slice(&req)?;

        let (namespace, method) = request.method.split_once(':').unwrap();

        let res = match (namespace, method) {
            ("mgmt", "set_script") => {
                let new_script: String = serde_json::from_str(request.params.get())?;
                program_nvs.set_blob("script", new_script.as_bytes())?;
                let _ = dbg!(script_engine.recompile(&new_script));
                rhai::Dynamic::UNIT
            }
            ("mgmt", "restart") => {
                esp_idf_svc::hal::reset::restart();
            }
            ("rpc", call) => script_engine.call(call, JsonFuncArgs(request.params)),
            _ => todo!(),
        };

        res_channel
            .send(serde_json::to_vec(&RpcResponse {
                id: request.id,
                result: res,
                error: rhai::Dynamic::UNIT,
            })?)
            .unwrap();
    }
}

fn connect_wifi(wifi: &mut BlockingWifi<EspWifi<'static>>) -> anyhow::Result<()> {
    let wifi_configuration: Configuration = Configuration::Client(ClientConfiguration {
        ssid: SSID.try_into().unwrap(),
        bssid: None,
        auth_method: AuthMethod::WPA2Personal,
        password: PASSWORD.try_into().unwrap(),
        channel: None,
        ..Default::default()
    });

    wifi.set_configuration(&wifi_configuration)?;

    wifi.start()?;
    info!("Wifi started");

    wifi.connect()?;
    info!("Wifi connected");

    wifi.wait_netif_up()?;
    info!("Wifi netif up");

    info!("{:?}", wifi.wifi().sta_netif().get_ip_info()?);
    Ok(())
}
