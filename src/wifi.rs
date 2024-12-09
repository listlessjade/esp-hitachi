use std::{fs::File, net::Ipv4Addr, rc::Rc};

use esp_idf_hal::sys::{
    esp, esp_eap_client_set_identity, esp_eap_client_set_password, esp_eap_client_set_username,
    esp_wifi_sta_enterprise_disable, esp_wifi_sta_enterprise_enable,
};
use esp_idf_svc::{
    eventloop::{EspEventLoop, System},
    wifi::{AuthMethod, BlockingWifi, EspWifi},
};
use log::info;
use parking_lot::lock_api::Mutex;
use serde::{Deserialize, Serialize};

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

#[derive(Clone)]
pub struct WifiManager {
    wifi: Rc<parking_lot::Mutex<BlockingWifi<EspWifi<'static>>>>,
}

static WIFI_CONFIG_PATH: &str = "/littlefs/wifi.json";

impl WifiManager {
    pub fn new(wifi: EspWifi<'static>, eloop: EspEventLoop<System>) -> Self {
        WifiManager {
            wifi: Rc::new(Mutex::new(BlockingWifi::wrap(wifi, eloop).unwrap())),
        }
    }
    pub fn read_config() -> anyhow::Result<Option<WifiConfig>> {
        if std::fs::exists(WIFI_CONFIG_PATH)? {
            let mut file = File::open(WIFI_CONFIG_PATH)?;
            let config = serde_json::from_reader(&mut file)?;
            Ok(Some(config))
        } else {
            Ok(None)
        }
    }

    pub fn store_config(&self, config: &WifiConfig) -> anyhow::Result<()> {
        let f = File::create(WIFI_CONFIG_PATH)?;
        serde_json::to_writer(f, config)?;
        Ok(())
    }

    pub fn set_config(&self, config: WifiConfig) -> anyhow::Result<()> {
        let mut wifi = self.wifi.lock();
        let mut esp_config_base = wifi.get_configuration()?;
        let esp_config = esp_config_base.as_client_conf_mut();
        esp_config.ssid = config.ssid;

        match config.authentication {
            WifiAuthentication::WPA2Personal { password } => {
                esp_config.auth_method = AuthMethod::WPA2Personal;
                esp_config.password = password;
                esp!(unsafe { esp_wifi_sta_enterprise_disable() })?;
            }
            WifiAuthentication::WPA2Enterprise {
                identity,
                username,
                password,
            } => {
                esp_config.auth_method = AuthMethod::WPA2Enterprise;
                esp!(unsafe {
                    esp_eap_client_set_identity(identity.as_ptr(), identity.as_bytes().len() as i32)
                })?;

                esp!(unsafe {
                    esp_eap_client_set_username(username.as_ptr(), username.as_bytes().len() as i32)
                })?;

                esp!(unsafe {
                    esp_eap_client_set_password(password.as_ptr(), password.as_bytes().len() as i32)
                })?;

                esp!(unsafe { esp_wifi_sta_enterprise_enable() })?;
            }
        }

        wifi.set_configuration(&esp_config_base)?;

        Ok(())
    }

    pub fn start(&self) -> anyhow::Result<()> {
        let mut wifi = self.wifi.lock();
        wifi.start()?;
        info!("Wifi started");

        wifi.connect()?;
        info!("Wifi connected");

        wifi.wait_netif_up()?;
        info!("Wifi netif up");

        info!("IP: {:?}", wifi.wifi().sta_netif().get_ip_info()?);

        Ok(())
    }

    pub fn stop(&self) -> anyhow::Result<()> {
        let mut wifi = self.wifi.lock();
        wifi.disconnect().unwrap();
        info!("Wifi disconnected");
        wifi.stop()?;
        info!("Wifi stopped");
        Ok(())
    }

    pub fn get_ip(&self) -> anyhow::Result<Ipv4Addr> {
        Ok(self.wifi.lock().wifi().sta_netif().get_ip_info()?.ip)
    }
}

// fn read_wifi_config() -> anyhow::Result<Option<WifiConfig>> {
// if std::fs::exists("/littlefs/wifi.json")? {
//     let mut file = File::open("/littlefs/wifi.json")?;
//     let config = serde_json::from_reader(&mut file)?;
//     Ok(Some(config))
// } else {
//     Ok(None)
// }
// }

// fn connect_wifi(wifi: &mut BlockingWifi<EspWifi<'static>>) -> anyhow::Result<()> {
//     if let Ok(mut config) = wifi.get_configuration() {
//         if let Some(stored_config) = read_wifi_config()? {
//             let live_config = config.as_client_conf_mut();
//             live_config.ssid = stored_config.ssid;
//             match stored_config.authentication {
//                 WifiAuthentication::WPA2Personal { password } => {
//                     live_config.auth_method = AuthMethod::WPA2Personal;
//                     live_config.password = password;
//                 }
//                 WifiAuthentication::WPA2Enterprise {
//                     identity,
//                     username,
//                     password,
//                 } => {

//                 }
//             }
//         } else {
//             return Ok(());
//         }

//         wifi.set_configuration(&config)?;

//     }

//     Ok(())
// }
