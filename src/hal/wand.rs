use esp_idf_svc::hal::ledc::LedcDriver;

use serde::{Deserialize, Serialize};
use thingbuf::mpsc::blocking::StaticSender;

use crate::{config::ConfigType, impl_conf_type};

#[derive(Serialize, Deserialize)]
#[repr(transparent)]
pub struct LightMappings {
    thresholds: [i64; 4],
}

impl Default for LightMappings {
    fn default() -> Self {
        LightMappings {
            thresholds: [-1, 25, 50, 75],
        }
    }
}

impl_conf_type!(LightMappings, "/littlefs/lights.json", LIGHT_MAPPINGS);

#[derive(Serialize, Deserialize, Clone, Copy, Default)]
pub struct Lights {
    pub mid_low: bool,
    pub mid_high: bool,
    pub top: bool,
    pub bottom: bool,
}

impl Lights {
    pub fn from_mapping(val: i64, mapping: &[i64; 4]) -> Self {
        Lights {
            mid_low: val > mapping[1],
            mid_high: val > mapping[2],
            top: val > mapping[3],
            bottom: val > mapping[0],
        }
    }

    pub fn write_into(&self, out: &mut String) {
        out.reserve(6);
        out.push(if self.mid_low { '1' } else { '0' });
        out.push(if self.mid_high { '1' } else { '0' });
        out.push(if self.top { '1' } else { '0' });
        out.push(if self.bottom { '1' } else { '0' });
        out.push_str("\r\n");
    }
}

pub struct Wand {
    pub percent: i64,
    pub driver: LedcDriver<'static>,
    pub uart_tx: StaticSender<Lights>,
}

impl Wand {
    pub fn get_percent(&mut self) -> i64 {
        self.percent
    }

    pub fn set_percent(&mut self, percent: i64) {
        let percent = if percent > 100 {
            percent
        } else if percent < 0 {
            0
        } else {
            percent
        };

        let max_duty = self.driver.get_max_duty();
        self.driver
            .set_duty(percent as u32 * max_duty / 100)
            .unwrap();
        self.percent = percent;

        let lights = LightMappings::CACHE
            .with(|val| Lights::from_mapping(percent, &val.borrow_mut().load().thresholds));

        let _ = self.uart_tx.send(lights);
    }
}
