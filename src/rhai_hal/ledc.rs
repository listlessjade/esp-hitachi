use std::rc::Rc;

use esp_idf_svc::hal::ledc::LedcDriver;

#[derive(Clone)]
pub struct PwmController {
    pub percent: i64,
    pub driver: Rc<parking_lot::Mutex<LedcDriver<'static>>>,
}

impl PwmController {
    fn get_percent(&mut self) -> i64 {
        self.percent
    }

    fn set_percent(&mut self, percent: i64) {
        let mut driver = self.driver.lock();
        let max_duty = driver.get_max_duty();
        driver.set_duty(percent as u32 * max_duty / 100).unwrap();
        self.percent = percent;
    }

    // fn fade(&mut self, new_percent: u8, duration_ms: u32)
}

impl rhai::CustomType for PwmController {
    fn build(mut builder: rhai::TypeBuilder<Self>) {
        builder
            .with_name("PwmController")
            .with_get("duty_percent", Self::get_percent)
            .with_fn("set_duty_percent", Self::set_percent);
    }
}
