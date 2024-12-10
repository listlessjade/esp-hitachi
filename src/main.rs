#![feature(maybe_uninit_slice)]

use std::{ffi::CString, rc::Rc, sync::{atomic::AtomicBool, Arc}};

use ble::run_ble;
use config::ConfigType;
use esp_idf_hal::{
    gpio, task::queue::Queue, uart::UartDriver
};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        ledc::{config::TimerConfig, LedcDriver, LedcTimerDriver},
        prelude::*,
        temp_sensor::{TempSensorConfig, TempSensorDriver},
    },
    mdns::EspMdns,
    nvs::EspDefaultNvsPartition,
    sys::{esp_nofail, esp_vfs_littlefs_conf_t, esp_vfs_littlefs_register},
    wifi::EspWifi,
};
#[cfg(feature = "usb_pd")]
use hal::husb238::Husb238Driver;
use hal::{hitachi_board::{HitachiConfig, Lights}, ledc::PwmController, uart::spawn_uart_thread};
use handlers::{lovense::{LovenseConfig, LovenseHandler}, rpc::RpcHandler};
use http::run_http;
use rpc::{ChannelOptions, MessageSource, ResponseTag, RpcCall, RpcResponse, REQUEST_QUEUE};
use serde::Serialize;
use wifi::{WifiConfig, WifiManager};
// use script::ScriptRunner;

mod ble;
mod hal;
mod handlers;
mod http;
mod rpc;
mod wifi;
mod config;

#[derive(Serialize, Copy, Clone)]
pub struct BuildInfo {
    pub git_branch: &'static str,
    pub git_commit: &'static str,
    pub built_at: &'static str,
    pub rustc_version: &'static str,
    pub crate_name: &'static str,
    pub crate_version: &'static str,
}

impl BuildInfo {
    pub const fn make() -> Self {
        Self {
            git_branch: env!("GIT_BRANCH"),
            git_commit: env!("GIT_COMMIT"),
            built_at: env!("BUILD_TIMESTAMP"),
            rustc_version: env!("RUSTC_VERSION"),
            crate_name: env!("CARGO_CRATE_NAME"),
            crate_version: env!("CARGO_PKG_VERSION"),
        }
    }
}

const BUILD_INFO: BuildInfo = BuildInfo::make();

pub static LAST_UART_MSG: parking_lot::Mutex<String> = parking_lot::Mutex::new(String::new());
pub static UPDATE_MAPPINGS: AtomicBool = AtomicBool::new(false);

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

    let (uart_requester, _uart_res_tx) = rpc::make_channel(
        req_tx.clone(),
        ChannelOptions {
            message_capacity: 4,
            min_buffer_size: 32,
            max_buffer_size: 64,
        },
    );

    esp_idf_svc::log::set_target_level("wifi", log::LevelFilter::Error).unwrap();
    esp_idf_svc::log::set_target_level("NimBLE", log::LevelFilter::Warn).unwrap();
    esp_idf_svc::log::set_target_level("wifi_init", log::LevelFilter::Warn).unwrap();
    esp_idf_svc::log::set_target_level("rhai", log::LevelFilter::Info).unwrap();

    log::info!("{}", include_str!("../banner.txt"));
    log::info!("Firmware built on {}", env!("BUILD_TIMESTAMP"));

    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let default_nvs = EspDefaultNvsPartition::take()?;

    let uart_tx = peripherals.pins.gpio22;
    let uart_rx = peripherals.pins.gpio23;

    let config = esp_idf_hal::uart::config::Config::new().baudrate(Hertz(9600));
    let uart: UartDriver<'static> = UartDriver::new(
        peripherals.uart1,
        uart_tx,
        uart_rx,
        Option::<gpio::Gpio0>::None,
        Option::<gpio::Gpio1>::None,
        &config,
    )?;

    let uart_queue = Arc::new(Queue::new(16));

    let (_uartrx_thread, _uarttx_thread, uart_tx) =
        spawn_uart_thread(uart_requester, uart, Arc::clone(&uart_queue));

    unsafe {
        let base_path = CString::new("/littlefs").unwrap();
        let storage = CString::new("storage").unwrap();

        let mut conf = esp_vfs_littlefs_conf_t {
            base_path: base_path.as_ptr(),
            partition_label: storage.as_ptr(),
            ..Default::default()
        };

        conf.set_format_if_mount_failed(1);
        conf.set_read_only(0);
        conf.set_grow_on_mount(1);

        esp_nofail!(esp_vfs_littlefs_register(&conf));
    }

    let wifi = WifiManager::new(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(default_nvs))?,
        sys_loop,
    );

    match WifiConfig::read() {
        Ok(Some(v)) => {
            if let Err(e) = wifi.set_config(v) {
                log::error!("Failed to set wifi config: {e}");
            }
        }
        Ok(None) => {
            log::warn!("Failed to set wifi config: non-existent")
        }
        Err(e) => {
            log::error!("Failed to read wifi config: {e}");
        }
    };

    if let Err(e) = wifi.start() {
        log::error!("Failed to start wifi: {e}");
    };

    let mut mdns = EspMdns::take()?;

    mdns.set_hostname("esp-magicwand")?;
    mdns.set_instance_name("Magic Wand [v0.1]")?;
    mdns.add_service(None, "_magicwandrpc", "_tcp", 8080, &[("version", "0")])?;

    let mut temp_sensor = TempSensorDriver::new(&TempSensorConfig::new(), peripherals.temp_sensor)?;
    temp_sensor.enable().unwrap();

    #[cfg(feature = "usb_pd")]
    let husb = Husb238Driver {
        i2c: Rc::new(parking_lot::Mutex::new(I2cDriver::new(
            peripherals.i2c0,
            peripherals.pins.gpio6,
            peripherals.pins.gpio7,
            &I2cConfig::default().baudrate(400_000.into()),
        )?)),
    };

    let timer_driver = LedcTimerDriver::new(
        peripherals.ledc.timer0,
        &TimerConfig::default().frequency(5.kHz().into()),
    )
    .unwrap();

    let ledc_driver = LedcDriver::new(
        peripherals.ledc.channel0,
        timer_driver,
        peripherals.pins.gpio10,
    )?;

    let pwm_controller = Rc::new(parking_lot::Mutex::new(PwmController {
        percent: 0,
        driver: ledc_driver,
        // uart_tx: uart_tx.clone(),
    }));

    let mut lovense_handler = LovenseHandler {
        pwm: Rc::clone(&pwm_controller),
        uart_tx: uart_tx.clone(),
        mappings: HitachiConfig::read()?.unwrap(),
        lovense_config: LovenseConfig::read()?.unwrap(),
    };

    // leds, pwm_t, pwm_d0, d_2, d_4
    uart_tx.send("1111,".to_string()).unwrap();
    let mut rpc_handler = RpcHandler::new(Rc::clone(&pwm_controller), temp_sensor, wifi, uart_tx.clone());

    let _ble_thread = std::thread::spawn(|| run_ble(ble_tx));
    let _http_server = run_http(http_tx, 8080);

    let mut hitachi_mappings = HitachiConfig::read()?.unwrap();

    loop {
        let message = req_rx.recv_ref().unwrap();
        let mut response_tag: ResponseTag = ResponseTag::Normal; // tags the response with a certain value at the end of the buffer

        let res_channel = match message.src {
            MessageSource::BleRpc => {
                response_tag = ResponseTag::BleRpc;
                &ble_res_tx
            }
            MessageSource::HttpRpc => &http_res_tx,
            MessageSource::BleLovense => {
                if UPDATE_MAPPINGS.swap(false, std::sync::atomic::Ordering::Relaxed) {
                    lovense_handler.mappings = HitachiConfig::read()?.unwrap();
                    lovense_handler.lovense_config = LovenseConfig::read()?.unwrap();
                    hitachi_mappings = lovense_handler.mappings;
                }

                lovense_handler.handle(
                    std::str::from_utf8(&message.buffer).unwrap(),
                    ble_res_tx.send_ref().unwrap(),
                );

                continue;
            }
            MessageSource::Uart => {
                if UPDATE_MAPPINGS.swap(false, std::sync::atomic::Ordering::Relaxed) {
                    lovense_handler.mappings = HitachiConfig::read()?.unwrap();
                    lovense_handler.lovense_config = LovenseConfig::read()?.unwrap();
                    hitachi_mappings = lovense_handler.mappings;
                }

                let msg = String::from_utf8_lossy(&message.buffer).into_owned();
                *LAST_UART_MSG.lock() = msg.clone();

                let msg_trimmed = msg.trim();
                if !msg_trimmed.starts_with("BUTTONS") {
                    continue;
                }

                if let Some((_, state)) = msg_trimmed.split_once(':') {
                    // state.by
                    let button_states = [state.as_bytes()[0] == b'0', state.as_bytes()[1] == b'0', state.as_bytes()[2] == b'0'];
                    
                    let delta = if button_states[1] {
                        Some(hitachi_mappings.button_mappings[0])
                    } else if button_states[2] {
                        Some(hitachi_mappings.button_mappings[1])
                    } else { None };

                    if let Some(delta) = delta {
                        let mut pwm = pwm_controller.lock();
                        let pct = pwm.get_percent() + delta;
                        pwm.set_percent(pct);
                                
                        let lights = Lights::from_mapping(pct, &hitachi_mappings.light_mappings);
                
                        let mut slot = uart_tx.send_ref().unwrap();
                        slot.clear();
                        lights.write_into(&mut slot);
                        drop(slot);
                    }
                }

                continue;
            }
        };

        let mut slot = res_channel.send_ref().unwrap();
        slot.tag = response_tag;

        let request: RpcCall<'_> = match serde_json::from_slice(&message.buffer) {
            Ok(v) => v,
            Err(e) => {
                log::error!("Invalid RPC request: {e}");
                serde_json::to_writer(
                    &mut slot.buffer,
                    &RpcResponse::new::<(), anyhow::Error>(
                        0,
                        anyhow::Result::Err(anyhow::anyhow!("Invalid JSON in RPC request: {}", e)),
                    ),
                )
                .unwrap();
                continue;
            }
        };

        if let Err(e) = rpc_handler.rpc_call(request, &mut slot.buffer) {
            log::error!("RPC handler error: {e}");
        }

        drop(slot);
    }
}
