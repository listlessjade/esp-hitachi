use esp_idf_svc::hal::ledc::LedcDriver;
use thingbuf::mpsc::blocking::StaticSender;

pub struct Lights {
    mid_low: bool,
    mid_high: bool,
    top: bool,
    bottom: bool,
}

// [0] = mid-low
// [1] = mid-high
// [2] = top
// [3] = bottom

impl Lights {
    pub fn write_into(&self, out: &mut String) {
        out.push(if self.mid_low { '1' } else { '0' });
        out.push(if self.mid_high { '1' } else { '0' });
        out.push(if self.top { '1' } else { '0' });
        out.push(if self.bottom { '1' } else { '0' });
    }
}

pub struct PwmController {
    pub percent: i64,
    pub driver: LedcDriver<'static>,
    pub uart_tx: StaticSender<String>,
}

impl PwmController {
    pub fn get_percent(&mut self) -> i64 {
        self.percent
    }

    pub fn set_percent(&mut self, percent: i64) {
        let max_duty = self.driver.get_max_duty();
        self.driver
            .set_duty(percent as u32 * max_duty / 100)
            .unwrap();
        self.percent = percent;

        let lights = Lights {
            bottom: true,
            mid_low: self.percent >= 25,
            mid_high: self.percent >= 50,
            top: self.percent >= 75,
        };

        let mut slot = self.uart_tx.send_ref().unwrap();
        slot.clear();
        lights.write_into(&mut slot);
        drop(slot);
        // if self.percent > 75 {
        //     // let mut slot = self.uart_tx.send_ref().unwrap();
        //     // slot.clear();
        //     // slot.push_str("1111,");
        // }
    }

    // fn fade(&mut self, new_percent: u8, duration_ms: u32)
}
