use esp_idf_svc::hal::ledc::LedcDriver;

pub struct PwmController {
    pub percent: i64,
    pub driver: LedcDriver<'static>,
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
    }

    // fn fade(&mut self, new_percent: u8, duration_ms: u32)
}
