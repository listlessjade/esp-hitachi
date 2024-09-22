use std::{cell::RefCell, rc::Rc};

use rhai::CustomType;
use smart_leds_trait::{SmartLedsWrite, RGB8};
use ws2812_esp32_rmt_driver::lib_smart_leds::Ws2812Esp32Rmt;

#[derive(Clone)]
pub struct RgbLed {
    pub driver: Rc<RefCell<Ws2812Esp32Rmt<'static>>>,
}

impl RgbLed {
    pub fn set_color(&mut self, r: i64, g: i64, b: i64) {
        self.driver
            .borrow_mut()
            .write([RGB8 {
                r: r.clamp(0, 255) as u8,
                g: g.clamp(0, 255) as u8,
                b: b.clamp(0, 255) as u8,
            }])
            .unwrap();
    }
}

impl CustomType for RgbLed {
    fn build(mut builder: rhai::TypeBuilder<Self>) {
        builder
            .with_name("RgbLed")
            .with_fn("set_color", Self::set_color);
    }
}
