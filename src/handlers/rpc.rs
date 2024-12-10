use std::{net::Ipv4Addr, rc::Rc};

use anyhow::anyhow;
use esp_idf_hal::{
    sys::{esp, esp_get_free_heap_size},
    temp_sensor::TempSensorDriver,
};
use esp_idf_svc::sys::esp_mac_type_t;
use serde::Serialize;
use thingbuf::mpsc::blocking::StaticSender;

#[cfg(feature = "usb_pd")]
use crate::hal::husb238::{Capabilities, Husb238Driver, Status};

use crate::{
    config::ConfigType, hal::{hitachi_board::{HitachiConfig, Lights}, ledc::PwmController}, rpc::{RpcCall, RpcResponse}, wifi::{WifiConfig, WifiManager}, BuildInfo, BUILD_INFO, LAST_UART_MSG, UPDATE_MAPPINGS
};
use esp_idf_hal::sys::{
    esp_mac_type_t_ESP_MAC_BASE, esp_mac_type_t_ESP_MAC_BT, esp_mac_type_t_ESP_MAC_WIFI_STA,
    esp_read_mac,
};

use super::lovense::LovenseConfig;

pub struct RpcHandler {
    sys: SysHandler,
    conn: ConnHandler,
    wand: WandHandler,
    uart: UartHandler,
}

impl RpcHandler {
    pub fn new(
        pwm: Rc<parking_lot::Mutex<PwmController>>,
        temp: TempSensorDriver<'static>,
        wifi: WifiManager,
        uart_tx: StaticSender<String>,
    ) -> Self {
        Self {
            sys: SysHandler { temp_sensor: temp },
            conn: ConnHandler { wifi },
            wand: WandHandler { pwm, uart_tx: uart_tx.clone(), mappings: HitachiConfig::read().unwrap().unwrap() },
            uart: UartHandler { uart_tx },
        }
    }

    pub fn rpc_call(&mut self, call: RpcCall<'_>, response: &mut Vec<u8>) -> anyhow::Result<()> {
        let (namespace, method) = call.method.split_once(':').unwrap();
        let res = match namespace {
            "sys" => self.sys.handle(call, method),
            "conn" => self.conn.handle(call, method),
            "wand" => self.wand.handle(call, method),
            "uart" => self.uart.handle(call, method),
            _ => RpcResponse::new::<(), _>(
                call.id,
                anyhow::Result::Err(anyhow!("Invalid namespace.")),
            ),
        };

        serde_json::to_writer(response, &res)?;
        Ok(())
    }
}

macro_rules! handle_methods {
    ($self:ident, $call_method:expr, $call:expr => withargs [$($method:ident);*] noargs [$($noargsmethod:ident);*]) => {
        {
            match $call_method {
                $(
                    // todo: don't unwrap here
                    stringify!($method) => {
                        let params = match serde_json::from_str($call.params.get()) {
                            Ok(v) => v,
                            Err(e) => return RpcResponse::new::<(), anyhow::Error>($call.id, anyhow::Result::Err(anyhow!("Invalid JSON for the arguments: {}", e))),
                        };

                        RpcResponse::new::<_, anyhow::Error>($call.id, $self.$method(params))
                    },
                )*
                $(
                    stringify!($noargsmethod) => RpcResponse::new::<_, anyhow::Error>($call.id, $self.$noargsmethod()),
                )*
                _ => RpcResponse::new::<(), anyhow::Error>($call.id, anyhow::Result::Err(anyhow!("Invalid method.")))
            }
        }
    }
}

pub struct SysHandler {
    temp_sensor: TempSensorDriver<'static>,
}

#[derive(Serialize)]
pub struct SystemInfo {
    temperature: f32,
    free_memory: u32,
}

impl SysHandler {
    pub fn handle(&mut self, call: RpcCall<'_>, method: &str) -> RpcResponse {
        handle_methods! (self, method, call => withargs [] noargs [health; restart; build_info])
    }

    pub fn build_info(&mut self) -> anyhow::Result<BuildInfo> {
        Ok(BUILD_INFO)
    }

    pub fn health(&mut self) -> anyhow::Result<SystemInfo> {
        Ok(SystemInfo {
            temperature: self.temp_sensor.get_celsius()?,
            free_memory: unsafe { esp_get_free_heap_size() },
        })
    }

    pub fn restart(&mut self) -> anyhow::Result<()> {
        esp_idf_svc::hal::reset::restart()
    }
}

#[derive(Serialize)]
pub struct Addresses {
    ip: Ipv4Addr,
    mac: MACAddresses,
}

#[derive(Serialize)]
pub struct MACAddresses {
    mac_base: String,
    mac_ble: String,
    mac_wifi: String,
}

impl MACAddresses {
    fn get_mode(mode: esp_mac_type_t) -> anyhow::Result<String> {
        let mut mac = [0u8; 6];
        esp!(unsafe { esp_read_mac(mac.as_mut_ptr(), mode) })?;
        Ok(format!(
            "{:<02X}:{:<02X}:{:<02X}:{:<02X}:{:<02X}:{:<02X}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
        ))
    }

    pub fn get() -> anyhow::Result<MACAddresses> {
        Ok(MACAddresses {
            mac_base: MACAddresses::get_mode(esp_mac_type_t_ESP_MAC_BASE)?,
            mac_ble: MACAddresses::get_mode(esp_mac_type_t_ESP_MAC_BT)?,
            mac_wifi: MACAddresses::get_mode(esp_mac_type_t_ESP_MAC_WIFI_STA)?,
        })
    }
}

pub struct ConnHandler {
    wifi: WifiManager,
}

impl ConnHandler {
    pub fn handle(&mut self, call: RpcCall<'_>, method: &str) -> RpcResponse {
        handle_methods! (self, method, call => withargs [set_wifi] noargs [addr])
    }

    pub fn set_wifi(&mut self, args: [WifiConfig; 1]) -> anyhow::Result<()> {
        let [conf] = args;
        conf.store()?;
        self.wifi.stop()?;
        self.wifi.set_config(conf)?;
        self.wifi.start()?;

        Ok(())
    }

    pub fn addr(&mut self) -> anyhow::Result<Addresses> {
        Ok(Addresses {
            ip: self.wifi.get_ip()?,
            mac: MACAddresses::get()?,
        })
    }
}

pub struct WandHandler {
    pub pwm: Rc<parking_lot::Mutex<PwmController>>,
    pub uart_tx: StaticSender<String>,
    pub mappings: HitachiConfig
}

impl WandHandler {
    pub fn handle(&mut self, call: RpcCall<'_>, method: &str) -> RpcResponse {
        handle_methods! (self, method, call => withargs [set_percent; update_mappings; update_lovense_mapping; set_button_increments; set_light_mappings] noargs [get_percent])
    }

    pub fn get_percent(&mut self) -> anyhow::Result<i64> {
        Ok(self.pwm.lock().get_percent())
    }

    pub fn set_percent(&mut self, args: [i64; 1]) -> anyhow::Result<()> {
        // let max_duty = 50;
        // let duty = max_duty as f64 * (args[0] as f64 / 100.0);
        // let duty: i64 = duty.trunc() as i64;

        // self.uart_tx.send("1111,".to_string()).unwrap();

        self.pwm.lock().set_percent(args[0]);

        let lights = Lights::from_mapping(args[0], &self.mappings.light_mappings);

        let mut slot = self.uart_tx.send_ref()?;
        slot.clear();
        lights.write_into(&mut slot);
        drop(slot);

        Ok(())
    }

    pub fn update_mappings(&mut self, args: [HitachiConfig; 1]) -> anyhow::Result<()> {
        args[0].store()?;
        self.mappings = args[0];

        UPDATE_MAPPINGS.store(true, std::sync::atomic::Ordering::Relaxed);

        Ok(())
    }

    pub fn set_button_increments(&mut self, args: [i64; 2]) -> anyhow::Result<()> {
        let mut conf = HitachiConfig::read()?.unwrap();
        conf.button_mappings = args;
        conf.store()?;
        Ok(())
    }

    pub fn set_light_mappings(&mut self, args: [i64; 4]) -> anyhow::Result<()> {
        let mut conf = HitachiConfig::read()?.unwrap();
        conf.light_mappings = args;
        conf.store()?;
        Ok(())
    }


    pub fn update_lovense_mapping(&mut self, args: [i64; 2]) -> anyhow::Result<()> {
        LovenseConfig { start: args[0], end: args[1] }.store()?;
        UPDATE_MAPPINGS.store(true, std::sync::atomic::Ordering::Relaxed);

        Ok(())
    }
}

pub struct UartHandler {
    pub uart_tx: StaticSender<String>,
}

impl UartHandler {
    pub fn handle(&mut self, call: RpcCall<'_>, method: &str) -> RpcResponse {
        handle_methods! (self, method, call => withargs [send] noargs [get_last])
    }

    pub fn get_last(&mut self) -> anyhow::Result<String> {
        Ok(LAST_UART_MSG.lock().clone())
    }

    pub fn send(&mut self, args: [String; 1]) -> anyhow::Result<()> {
        self.uart_tx.send(args[0].clone()).unwrap();
        Ok(())
    }
}
