#![feature(maybe_uninit_slice)]

use std::{ffi::CString, fs::File, rc::Rc};

use ble::run_ble;
use embedded_svc::wifi::Wifi;
use esp_idf_hal::{
    gpio,
    sys::{
        esp, esp_eap_client_set_identity, esp_eap_client_set_password, esp_eap_client_set_username,
        esp_wifi_sta_enterprise_enable,
    },
    uart::UartDriver,
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
    wifi::{AuthMethod, BlockingWifi, EspWifi},
};
#[cfg(feature = "usb_pd")]
use hal::husb238::Husb238Driver;
use hal::{ledc::PwmController, uart::spawn_uart_thread};
use handlers::{lovense::LovenseHandler, rpc::RpcHandler};
use http::run_http;
use log::info;
use rpc::{ChannelOptions, MessageSource, ResponseTag, RpcCall, RpcResponse, REQUEST_QUEUE};
use serde::{Deserialize, Serialize};
// use script::ScriptRunner;

mod ble;
mod hal;
mod handlers;
mod http;
mod rpc;

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

    let (uart_requester, uart_res_tx) = rpc::make_channel(
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
    log::info!("Firmware built on {}", compile_time::datetime_str!());

    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let default_nvs = EspDefaultNvsPartition::take()?;

    let uart_tx = peripherals.pins.gpio22;
    let uart_rx = peripherals.pins.gpio23;

    println!("Starting UART loopback test");
    let config = esp_idf_hal::uart::config::Config::new().baudrate(Hertz(9600));
    let uart: UartDriver<'static> = UartDriver::new(
        peripherals.uart1,
        uart_tx,
        uart_rx,
        Option::<gpio::Gpio0>::None,
        Option::<gpio::Gpio1>::None,
        &config,
    )?;

    // let uart_queue = Arc::new(Queue::new(16));

    let (_uartrx_thread, _uarttx_thread, uart_tx) =
        spawn_uart_thread(uart_requester, uart /*, Arc::clone(&uart_queue)*/);

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

    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(default_nvs))?,
        sys_loop,
    )?;

    let _ = connect_wifi(&mut wifi);

    // let _sntp = EspSntp::new_default()?;

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
        uart_tx: uart_tx.clone(),
    }));

    let mut lovense_handler = LovenseHandler {
        pwm: Rc::clone(&pwm_controller),
    };

    // leds, pwm_t, pwm_d0, d_2, d_4
    uart_tx.send("1111,".to_string()).unwrap();
    let mut rpc_handler = RpcHandler::new(Rc::clone(&pwm_controller), temp_sensor, wifi, uart_tx);

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
            MessageSource::BleLovense => {
                lovense_handler.handle(
                    std::str::from_utf8(&message.buffer).unwrap(),
                    ble_res_tx.send_ref().unwrap(),
                );

                continue;
            }
            MessageSource::Uart => {
                let msg = String::from_utf8_lossy(&message.buffer).into_owned();
                *LAST_UART_MSG.lock() = msg.clone();

                let msg_trimmed = msg.trim();
                if !msg_trimmed.starts_with("BUTTONS") {
                    continue;
                }

                if let Some((_, state)) = msg_trimmed.split_once(':') {
                    if state.chars().nth(2) == Some('0') {
                        pwm_controller.lock().set_percent(100);
                    } else if state.chars().nth(1) == Some('0') {
                        pwm_controller.lock().set_percent(0);
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

#[derive(Serialize, Deserialize)]
pub struct WifiConfig {
    pub ssid: heapless::String<32>,
    pub authentication: WifiAuthentication,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WifiAuthentication {
    #[serde(alias = "personal")]
    WPA2Personal { password: heapless::String<64> },
    #[serde(alias = "enterprise")]
    WPA2Enterprise {
        identity: String,
        username: String,
        password: String,
    },
}

fn read_wifi_config() -> anyhow::Result<Option<WifiConfig>> {
    if std::fs::exists("/littlefs/wifi.json")? {
        let mut file = File::open("/littlefs/wifi.json")?;
        let config = serde_json::from_reader(&mut file)?;
        Ok(Some(config))
    } else {
        Ok(None)
    }
}

fn connect_wifi(wifi: &mut BlockingWifi<EspWifi<'static>>) -> anyhow::Result<()> {
    if let Ok(mut config) = wifi.get_configuration() {
        if let Some(stored_config) = read_wifi_config()? {
            let live_config = config.as_client_conf_mut();
            live_config.ssid = stored_config.ssid;
            match stored_config.authentication {
                WifiAuthentication::WPA2Personal { password } => {
                    live_config.auth_method = AuthMethod::WPA2Personal;
                    live_config.password = password;
                }
                WifiAuthentication::WPA2Enterprise {
                    identity,
                    username,
                    password,
                } => {
                    live_config.auth_method = AuthMethod::WPA2Enterprise;
                    esp!(unsafe {
                        esp_eap_client_set_identity(
                            identity.as_ptr(),
                            identity.as_bytes().len() as i32,
                        )
                    })?;

                    esp!(unsafe {
                        esp_eap_client_set_username(
                            username.as_ptr(),
                            username.as_bytes().len() as i32,
                        )
                    })?;

                    esp!(unsafe {
                        esp_eap_client_set_password(
                            password.as_ptr(),
                            password.as_bytes().len() as i32,
                        )
                    })?;

                    esp!(unsafe { esp_wifi_sta_enterprise_enable() })?;
                }
            }
        } else {
            return Ok(());
        }

        wifi.set_configuration(&config)?;

        wifi.start().unwrap();
        info!("Wifi started");

        wifi.connect().unwrap();
        info!("Wifi connected");

        wifi.wait_netif_up().unwrap();
        info!("Wifi netif up");

        info!("{:?}", wifi.wifi().sta_netif().get_ip_info()?);
    }

    Ok(())
}
