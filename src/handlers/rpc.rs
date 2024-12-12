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
    config::ConfigType,
    hal::wand::{Lights, Wand},
    rpc::{MessageRecycler, MessageSource, RequestMessage, RpcCall, RpcResponse},
    wifi::{WifiConfig, WifiManager},
    BuildInfo, BUILD_INFO, LAST_UART_MSG,
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
        pwm: Rc<parking_lot::Mutex<Wand>>,
        temp: TempSensorDriver<'static>,
        wifi: WifiManager,
        uart_tx: StaticSender<Lights>,
        req_tx: StaticSender<RequestMessage, MessageRecycler>
    ) -> Self {
        Self {
            sys: SysHandler { temp_sensor: temp, req_tx },
            conn: ConnHandler { wifi },
            wand: WandHandler { pwm },
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
    req_tx: StaticSender<RequestMessage, MessageRecycler>
}

#[derive(Serialize)]
pub struct SystemInfo {
    temperature: f32,
    free_memory: u32,
}

impl SysHandler {
    pub fn handle(&mut self, call: RpcCall<'_>, method: &str) -> RpcResponse {
        handle_methods! (self, method, call => withargs [fake_uart] noargs [health; restart; build_info])
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

    pub fn fake_uart(&mut self, args: [String; 1]) -> anyhow::Result<()> {
        let [s] = args;

        let mut slot = self.req_tx.send_ref().unwrap();
        slot.buffer.append(&mut s.into_bytes());
        slot.buffer.extend_from_slice(b"\r\n");
        slot.src = MessageSource::Uart;
        Ok(())
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
        let conf = conf.store()?;
        self.wifi.stop()?;
        self.wifi.set_config(&conf)?;
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
    pub pwm: Rc<parking_lot::Mutex<Wand>>,
}

impl WandHandler {
    pub fn handle(&mut self, call: RpcCall<'_>, method: &str) -> RpcResponse {
        handle_methods! (self, method, call => withargs [set_percent; update_lovense_mapping] noargs [get_percent])
    }

    pub fn get_percent(&mut self) -> anyhow::Result<i64> {
        Ok(self.pwm.lock().get_percent())
    }

    pub fn set_percent(&mut self, args: [i64; 1]) -> anyhow::Result<()> {
        self.pwm.lock().set_percent(args[0]);

        Ok(())
    }

    pub fn update_lovense_mapping(&mut self, args: [i64; 2]) -> anyhow::Result<()> {
        LovenseConfig {
            start: args[0],
            end: args[1],
        }
        .store()?;

        Ok(())
    }
}

pub struct UartHandler {
    pub uart_tx: StaticSender<Lights>,
}

impl UartHandler {
    pub fn handle(&mut self, call: RpcCall<'_>, method: &str) -> RpcResponse {
        handle_methods! (self, method, call => withargs [send] noargs [get_last])
    }

    pub fn get_last(&mut self) -> anyhow::Result<String> {
        Ok(LAST_UART_MSG.lock().clone())
    }

    pub fn send(&mut self, args: [bool; 4]) -> anyhow::Result<()> {
        self.uart_tx
            .send(Lights {
                mid_low: args[1],
                mid_high: args[2],
                top: args[3],
                bottom: args[0],
            })
            .unwrap();
        Ok(())
    }
}
