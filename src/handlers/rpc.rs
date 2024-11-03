use std::{net::Ipv4Addr, rc::Rc, str::FromStr};

use anyhow::anyhow;
use esp_idf_hal::{
    sys::{esp, esp_get_free_heap_size},
    temp_sensor::TempSensorDriver,
};
use esp_idf_svc::sys::esp_mac_type_t;
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};
use serde::Serialize;

use crate::{
    hal::{
        husb238::{Capabilities, Husb238Driver, Status},
        ledc::PwmController,
    },
    rpc::{RpcCall, RpcResponse},
};
use esp_idf_hal::sys::{
    esp_mac_type_t_ESP_MAC_BASE, esp_mac_type_t_ESP_MAC_BT, esp_mac_type_t_ESP_MAC_WIFI_STA,
    esp_read_mac,
};

pub struct RpcHandler {
    pub pwm: Rc<parking_lot::Mutex<PwmController>>,
    pub temp_sensor: TempSensorDriver<'static>,
    pub husb: Husb238Driver,
    pub wifi: BlockingWifi<EspWifi<'static>>,
}

#[derive(Serialize)]
pub struct SystemInfo {
    temperature: f32,
    free_memory: u32,
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

#[derive(Serialize)]
pub struct PowerStatus {
    capabilities: Capabilities,
    status: Status,
}

impl RpcHandler {
    pub fn rpc_call(&mut self, call: RpcCall<'_>, response: &mut Vec<u8>) {
        let (namespace, method) = call.method.split_once(':').unwrap();
        let res = match (namespace, method) {
            ("mgmt", "health") => RpcResponse::new(call.id, self.mgmt_health()),
            ("mgmt", "addr") => RpcResponse::new(call.id, self.mgmt_addr()),
            ("mgmt", "power") => RpcResponse::new(call.id, self.mgmt_power()),
            ("mgmt", "restart") => self.mgmt_restart(),
            ("mgmt", "set_wifi") => RpcResponse::new(
                call.id,
                self.mgmt_set_wifi(serde_json::from_str(call.params.get()).unwrap()),
            ),
            ("wand", "set_percent") => RpcResponse::new(
                call.id,
                self.wand_set_percent(serde_json::from_str(call.params.get()).unwrap()),
            ),
            ("wand", "get_percent") => RpcResponse::new(call.id, self.wand_get_percent()),
            (_, _) => {
                RpcResponse::new::<(), _>(call.id, anyhow::Result::Err(anyhow!("Invalid method.")))
            }
        };

        serde_json::to_writer(response, &res).unwrap();
        // let res: RpcResponse = RpcResponse::new(call.id, res)
    }
}

impl RpcHandler {
    pub fn wand_get_percent(&mut self) -> anyhow::Result<i64> {
        Ok(self.pwm.lock().get_percent())
    }

    pub fn wand_set_percent(&mut self, args: [i64; 1]) -> anyhow::Result<()> {
        self.pwm.lock().set_percent(args[0]);
        Ok(())
    }

    pub fn mgmt_health(&mut self) -> anyhow::Result<SystemInfo> {
        Ok(SystemInfo {
            temperature: self.temp_sensor.get_celsius()?,
            free_memory: unsafe { esp_get_free_heap_size() },
        })
    }

    pub fn mgmt_addr(&mut self) -> anyhow::Result<Addresses> {
        Ok(Addresses {
            ip: self.wifi.wifi().sta_netif().get_ip_info()?.ip,
            mac: MACAddresses::get()?,
        })
    }

    pub fn mgmt_power(&mut self) -> anyhow::Result<PowerStatus> {
        Ok(PowerStatus {
            capabilities: self.husb.get_capabilities()?,
            status: self.husb.get_status()?,
        })
    }

    pub fn mgmt_restart(&mut self) -> ! {
        esp_idf_svc::hal::reset::restart()
    }

    pub fn mgmt_set_wifi(&mut self, args: Vec<String>) -> anyhow::Result<()> {
        let mut config = Configuration::Client(ClientConfiguration::default());

        config.as_client_conf_mut().ssid = heapless::String::from_str(&args[0]).unwrap();
        config.as_client_conf_mut().password = heapless::String::from_str(&args[1]).unwrap();
        config.as_client_conf_mut().auth_method = AuthMethod::WPA2Personal;

        self.wifi.set_configuration(&config)?;
        Ok(())
    }
}
