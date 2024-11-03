use std::{ops::Range, rc::Rc};

use thingbuf::mpsc::blocking::SendRef;

use crate::{
    hal::ledc::PwmController,
    rpc::{ResponseMessage, ResponseTag},
};

fn map_range(lhs: Range<i64>, rhs: Range<i64>, val: i64) -> i64 {
    rhs.start + ((val - lhs.start) * (rhs.end - rhs.start) / (lhs.end - lhs.start))
}

pub struct LovenseHandler {
    pub pwm: Rc<parking_lot::Mutex<PwmController>>,
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
                let target_range = 50..100;

                let strength = args[1].parse::<i64>().unwrap();
                let mapped = if strength > 0 {
                    map_range(lovense_range, target_range, strength)
                } else {
                    0
                };

                self.pwm.lock().set_percent(mapped);

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
