#![feature(maybe_uninit_slice)]

use std::{cell::RefCell, ffi::CString, rc::Rc, str::FromStr, time::Instant};

use ble::run_ble;
use embedded_svc::wifi::Wifi;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        ledc::{config::TimerConfig, LedcDriver, LedcTimerDriver},
        prelude::*,
        temp_sensor::{TempSensorConfig, TempSensorDriver},
    },
    mdns::EspMdns,
    nvs::EspDefaultNvsPartition,
    sys::{
        esp, esp_mac_type_t_ESP_MAC_BT, esp_nofail, esp_read_mac, esp_vfs_spiffs_conf_t,
        esp_vfs_spiffs_register,
    },
    timer::EspTaskTimerService,
    wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};
use http::run_http;
use log::info;
use parking_lot::Mutex;
use rhai::Dynamic;
use rhai_hal::{health::SystemHealth, ledc::PwmController, timer::CallTimer, ws2812::RgbLed};
use rpc::{
    ChannelOptions, JsonFuncArgs, MessageSource, ResponseTag, RpcCall, RpcResponse, REQUEST_QUEUE,
};
use script::{LovenseArgs, ScriptRunner};
use ws2812_esp32_rmt_driver::LedPixelEsp32Rmt;
// use script::ScriptRunner;
mod script;

mod ble;
mod http;
mod rhai_hal;
mod rpc;

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let (req_tx, req_rx) = REQUEST_QUEUE.split();

    let (ble_tx, ble_res_tx) = rpc::make_channel(
        req_tx.clone(),
        ChannelOptions {
            message_capacity: 8,
            min_buffer_size: 32,
            max_buffer_size: 64,
        },
    );
    let (http_tx, http_res_tx) = rpc::make_channel(
        req_tx.clone(),
        ChannelOptions {
            message_capacity: 4,
            min_buffer_size: 64,
            max_buffer_size: 512,
        },
    );

    esp_idf_svc::log::set_target_level("wifi", log::LevelFilter::Error).unwrap();
    esp_idf_svc::log::set_target_level("NimBLE", log::LevelFilter::Warn).unwrap();
    esp_idf_svc::log::set_target_level("wifi_init", log::LevelFilter::Warn).unwrap();
    esp_idf_svc::log::set_target_level("rhai", log::LevelFilter::Info).unwrap();

    log::info!("{}", include_str!("../banner.txt"));
    log::info!("Firmware built on {}", compile_time::datetime_str!());

    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let default_nvs = EspDefaultNvsPartition::take()?;

    unsafe {
        let base_path = CString::new("/spiffs").unwrap();
        let storage = CString::new("programs").unwrap();

        let conf = esp_vfs_spiffs_conf_t {
            base_path: base_path.as_ptr(),
            partition_label: storage.as_ptr(),
            max_files: 1,
            format_if_mount_failed: true,
        };

        esp_nofail!(esp_vfs_spiffs_register(&conf));
    }

    let led_driver = RgbLed {
        driver: Rc::new(RefCell::new(LedPixelEsp32Rmt::new(
            peripherals.rmt.channel0,
            peripherals.pins.gpio8,
        )?)),
    };

    let timer_driver = LedcTimerDriver::new(
        peripherals.ledc.timer0,
        &TimerConfig::default().frequency(5.kHz().into()),
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

    let _ = connect_wifi(&mut wifi);

    let mut mac = [0u8; 6];
    esp!(unsafe { esp_read_mac(mac.as_mut_ptr(), esp_mac_type_t_ESP_MAC_BT) })?;
    let mac_addr = format!(
        "{:<02X}{:<02X}{:<02X}{:<02X}{:<02X}{:<02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );

    // if let Some(program) = program_nvs.get_ra

    let mut mdns = EspMdns::take()?;

    mdns.set_hostname("esp-magicwand")?;
    mdns.set_instance_name("Magic Wand [v0.1]")?;
    mdns.add_service(None, "_magicwandrpc", "_tcp", 8080, &[("version", "0")])?;

    // std::thread::scope(move |s| {
    let mut script_engine = ScriptRunner::new(ble_res_tx.clone());

    script_engine.insert_builtin(
        "pwm",
        PwmController {
            percent: 0,
            driver: Rc::new(Mutex::new(driver)),
        },
    );

    script_engine.insert_builtin("mac_address", mac_addr);

    let timer_service = EspTaskTimerService::new()?;

    let timer_tx = req_tx.clone();

    script_engine.insert_builtin(
        "timer",
        CallTimer {
            interval: 0,
            inner: Rc::new(timer_service.timer(move || {
                let mut slot = timer_tx.send_ref().unwrap();
                slot.src = MessageSource::Timer;
                drop(slot);
                // timer_tx.send(()).unwrap();
            })?),
        },
    );

    let mut temp_sensor = TempSensorDriver::new(&TempSensorConfig::new(), peripherals.temp_sensor)?;

    temp_sensor.enable().unwrap();

    script_engine.insert_builtin(
        "system",
        SystemHealth {
            temp_sensor: Rc::new(temp_sensor),
        },
    );

    script_engine.insert_builtin("led", led_driver);

    if std::fs::exists("/spiffs/script.rhai")? {
        script_engine.recompile(&std::fs::read_to_string("/spiffs/script.rhai")?)?;
    }

    // let (ws_tx, ws_rx) = rpc::make_channel(4);

    let _ble_thread = std::thread::spawn(|| run_ble(ble_tx));
    let _http_server = run_http(http_tx, 8080);

    loop {
        let message = req_rx.recv_ref().unwrap();
        let mut response_tag: ResponseTag = ResponseTag::Normal; // tags the response with a certain value at the end of the buffer

        let res_channel = match message.src {
            MessageSource::BleRpc => {
                response_tag = ResponseTag::BleRpc;
                &ble_res_tx
            }
            MessageSource::HttpRpc => &http_res_tx,
            MessageSource::Timer => {
                let _ = script_engine.call::<rhai::Dynamic>(
                    "tick",
                    (rhai::Dynamic::from_timestamp(Instant::now()),),
                );
                continue;
            }
            MessageSource::BleLovense => {
                let Some(lovense_args) =
                    LovenseArgs::new(std::str::from_utf8(&message.buffer).unwrap())
                else {
                    continue;
                };

                let Ok(res) = script_engine.call::<rhai::Dynamic>("lovense", lovense_args) else {
                    continue;
                };
                let mut res: Vec<Dynamic> = res.into_array().unwrap();

                let mut send_slot = ble_res_tx.send_ref().unwrap();
                let mut res_iter = res.iter_mut();

                if let Some(first) = res_iter.next() {
                    send_slot.buffer.extend_from_slice(
                        first.take().into_immutable_string().unwrap().as_bytes(),
                    );
                }

                for arg in res_iter {
                    send_slot.buffer.push(b':');
                    send_slot
                        .buffer
                        .extend_from_slice(arg.take().into_immutable_string().unwrap().as_bytes());
                }

                // for lovense messages, we add a last byte to the array indicating it's a lovense one
                send_slot.tag = ResponseTag::Lovense;

                drop(send_slot);

                continue;
            }
        };

        let request: RpcCall<'_> = serde_json::from_slice(&message.buffer)?;

        let (namespace, method) = request.method.split_once(':').unwrap();

        let (res, err) = match (namespace, method) {
            ("mgmt", "recompile") => {
                script_engine.recompile(&std::fs::read_to_string("/spiffs/script.rhai")?)?;
                (rhai::Dynamic::TRUE, None)
            }
            ("mgmt", "restart") => {
                esp_idf_svc::hal::reset::restart();
            }
            ("mgmt", "set_wifi") => {
                let args: Vec<String> = serde_json::from_str(request.params.get())?;
                let mut config = Configuration::Client(ClientConfiguration::default());
                config.as_client_conf_mut().ssid = heapless::String::from_str(&args[0]).unwrap();
                config.as_client_conf_mut().password =
                    heapless::String::from_str(&args[1]).unwrap();
                config.as_client_conf_mut().auth_method = AuthMethod::WPA2Personal;
                match wifi.set_configuration(&config) {
                    Ok(_) => (rhai::Dynamic::TRUE, None),
                    Err(e) => (rhai::Dynamic::FALSE, Some(e.to_string())),
                }
            }
            ("rpc", call) => match script_engine.call(call, JsonFuncArgs(request.params)) {
                Ok(v) => (v, None),
                Err(e) => (rhai::Dynamic::UNIT, Some(e.to_string())),
            },
            _ => (rhai::Dynamic::UNIT, Some("invalid method call".to_owned())),
        };

        let mut slot = res_channel.send_ref().unwrap();
        slot.tag = response_tag;
        serde_json::to_writer(
            &mut slot.buffer,
            &RpcResponse {
                id: request.id,
                result: res,
                error: err,
            },
        )?;

        drop(slot);
    }
}

fn connect_wifi(wifi: &mut BlockingWifi<EspWifi<'static>>) -> anyhow::Result<()> {
    if let Ok(config) = wifi.get_configuration() {
        wifi.set_configuration(&config)?;

        wifi.start()?;
        info!("Wifi started");

        wifi.connect()?;
        info!("Wifi connected");

        wifi.wait_netif_up()?;
        info!("Wifi netif up");

        info!("{:?}", wifi.wifi().sta_netif().get_ip_info()?);
    }

    Ok(())
}
