use std::{ops::Range, rc::Rc};

use serde::{Deserialize, Serialize};
use thingbuf::mpsc::blocking::{SendRef, StaticSender};

use crate::{
    config::ConfigType, hal::{hitachi_board::{HitachiConfig, Lights}, ledc::PwmController}, rpc::{ResponseMessage, ResponseTag}
};

fn map_range(lhs: Range<i64>, rhs: Range<i64>, val: i64) -> i64 {
    rhs.start + ((val - lhs.start) * (rhs.end - rhs.start) / (lhs.end - lhs.start))
}

#[derive(Serialize, Deserialize)]
pub struct LovenseConfig {
    pub start: i64,
    pub end: i64
}

impl ConfigType for LovenseConfig {
    const PATH: &str = "/littlefs/lovense.json";
    const HAS_DEFAULT: bool = true;

    fn default() -> Option<Self> {
        Some(LovenseConfig { start: 65, end: 100 })
    }
}

pub struct LovenseHandler {
    pub pwm: Rc<parking_lot::Mutex<PwmController>>,
    pub uart_tx: StaticSender<String>,
    pub mappings: HitachiConfig,
    pub lovense_config: LovenseConfig
}

impl LovenseHandler {
    pub fn handle(&mut self, args: &str, mut send_slot: SendRef<'_, ResponseMessage>) {
        let Some(msg_end) = args.find(';') else {
            send_slot.tag = ResponseTag::Discard;
            return;
        };

        let args: Vec<&str> = args[..msg_end].split_terminator(':').collect();

        log::info!(target: "lovense", "Lovense Command: {args:?}");

        let res: &[&str] = match args[0] {
            "Battery" => &["100"],
            "Status" => &["2"],
            "GetLight" => &["Light", "1"],
            "Vibrate" => {
                let lovense_range = 0..20;
                let target_range = self.lovense_config.start..self.lovense_config.end;

                let strength = args[1].parse::<i64>().unwrap();
                let mapped = if strength > 0 {
                    map_range(lovense_range, target_range, strength)
                } else {
                    0
                };

                self.pwm.lock().set_percent(mapped);

                let lights = Lights::from_mapping(mapped, &self.mappings.light_mappings);
        
                let mut slot = self.uart_tx.send_ref().unwrap();
                slot.clear();
                lights.write_into(&mut slot);
                drop(slot);
                // let max_duty = 999;
                // let duty = max_duty as f64 * (mapped as f64 / 100.0);
                // let duty: i64 = duty.trunc() as i64;

                // if mapped > 0 {
                //     self.uart_tx.send("1111,".to_string()).unwrap();
                // } else {
                //     self.uart_tx.send("1111,".to_string()).unwrap();
                // }

                &[]
            }
            _ => &[],
        };

        let mut res_iter = res.iter();

        if let Some(first) = res_iter.next() {
            send_slot.buffer.extend_from_slice(first.as_bytes());
        }

        for arg in res_iter {
            send_slot.buffer.push(b':');
            send_slot.buffer.extend_from_slice(arg.as_bytes());
        }

        // for lovense messages, we add a last byte to the array indicating it's a lovense one
        send_slot.tag = ResponseTag::Lovense;

        drop(send_slot);
    }
}
