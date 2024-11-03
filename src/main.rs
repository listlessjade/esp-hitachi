#![feature(maybe_uninit_slice)]

use std::{ffi::CString, rc::Rc};

use ble::run_ble;
use embedded_svc::wifi::Wifi;
use esp_idf_hal::i2c::{I2cConfig, I2cDriver};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        ledc::{config::TimerConfig, LedcDriver, LedcTimerDriver},
        prelude::*,
        temp_sensor::{TempSensorConfig, TempSensorDriver},
    },
    mdns::EspMdns,
    nvs::EspDefaultNvsPartition,
    sys::{esp_nofail, esp_vfs_spiffs_conf_t, esp_vfs_spiffs_register},
    wifi::{BlockingWifi, EspWifi},
};
use hal::{husb238::Husb238Driver, ledc::PwmController};
use handlers::{lovense::LovenseHandler, rpc::RpcHandler};
use http::run_http;
use log::info;
use rpc::{ChannelOptions, MessageSource, ResponseTag, RpcCall, REQUEST_QUEUE};
// use script::ScriptRunner;

mod ble;
mod hal;
mod handlers;
mod http;
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
            max_files: 5,
            format_if_mount_failed: true,
        };

        esp_nofail!(esp_vfs_spiffs_register(&conf));
    }

    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(default_nvs))?,
        sys_loop,
    )?;

    let _ = connect_wifi(&mut wifi);

    let mut mdns = EspMdns::take()?;

    mdns.set_hostname("esp-magicwand")?;
    mdns.set_instance_name("Magic Wand [v0.1]")?;
    mdns.add_service(None, "_magicwandrpc", "_tcp", 8080, &[("version", "0")])?;

    let mut temp_sensor = TempSensorDriver::new(&TempSensorConfig::new(), peripherals.temp_sensor)?;
    temp_sensor.enable().unwrap();

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
    }));

    let mut lovense_handler = LovenseHandler {
        pwm: Rc::clone(&pwm_controller),
    };

    let mut rpc_handler = RpcHandler {
        pwm: pwm_controller,
        temp_sensor,
        husb,
        wifi,
    };

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
        };

        let request: RpcCall<'_> = serde_json::from_slice(&message.buffer)?;
        let mut slot = res_channel.send_ref().unwrap();
        slot.tag = response_tag;
        rpc_handler.rpc_call(request, &mut slot.buffer);

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
