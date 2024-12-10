use std::rc::Rc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use thingbuf::mpsc::blocking::StaticSender;

use crate::config::ConfigType;

#[derive(Serialize, Deserialize)]
pub struct Lights {
    pub mid_low: bool,
    pub mid_high: bool,
    pub top: bool,
    pub bottom: bool,
}

// [0] = mid-low
// [1] = mid-high
// [2] = top
// [3] = bottom

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
        out.push(if self.mid_low { '1' } else { '0' });
        out.push(if self.mid_high { '1' } else { '0' });
        out.push(if self.top { '1' } else { '0' });
        out.push(if self.bottom { '1' } else { '0' });
    }
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct HitachiConfig {
    pub light_mappings: [i64; 4], // [bottom, mid-low, mid-high, top] <- thresholds for lights based on PWM
    pub button_mappings: [i64; 2] // [bottom, top] <- how much to increase/decrease PWM pct by
}

impl ConfigType for HitachiConfig {
    const PATH: &str = "/littlefs/hitachi_mappings.json";
    const HAS_DEFAULT: bool = true;


    fn default() -> Option<Self> { 
        Some(Self {
            light_mappings: [0, 25, 50, 75],
            button_mappings: [-100, 100]
        })
     }
    
}

