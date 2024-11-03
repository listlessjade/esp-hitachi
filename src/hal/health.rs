use esp_idf_hal::sys::{heap_caps_get_largest_free_block, MALLOC_CAP_DEFAULT};
use esp_idf_svc::{
    hal::temp_sensor::TempSensorDriver,
    sys::{esp_get_free_heap_size, esp_get_minimum_free_heap_size},
};

pub struct SystemHealth {
    pub temp_sensor: TempSensorDriver<'static>,
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

    pub fn get_largest_free_block(&mut self) -> usize {
        unsafe { heap_caps_get_largest_free_block(MALLOC_CAP_DEFAULT) }
    }
}
