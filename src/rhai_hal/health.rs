use std::rc::Rc;

use esp_idf_svc::{
    hal::temp_sensor::TempSensorDriver,
    sys::{esp_get_free_heap_size, esp_get_minimum_free_heap_size},
};

#[derive(Clone)]
pub struct SystemHealth {
    pub temp_sensor: Rc<TempSensorDriver<'static>>,
}

impl SystemHealth {
    pub fn get_celsius(&mut self) -> f32 {
        self.temp_sensor.get_celsius().unwrap()
    }

    pub fn get_free_memory(&mut self) -> u32 {
        unsafe { esp_get_free_heap_size() }
    }

    pub fn get_lifetime_minimum_free_memory(&mut self) -> u32 {
        unsafe { esp_get_minimum_free_heap_size() }
    }
}

impl rhai::CustomType for SystemHealth {
    fn build(mut builder: rhai::TypeBuilder<Self>) {
        builder
            .with_name("SystemHealth")
            .with_fn("chip_temperature", Self::get_celsius)
            .with_fn("free_memory", Self::get_free_memory)
            .with_fn(
                "minimum_free_memory",
                Self::get_lifetime_minimum_free_memory,
            );
    }
}
