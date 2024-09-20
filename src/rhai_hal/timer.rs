use std::{rc::Rc, time::Duration};

use esp_idf_svc::timer::EspTimer;
use rhai::CustomType;

#[derive(Clone)]
pub struct CallTimer {
    pub interval: i64,
    pub inner: Rc<EspTimer<'static>>,
}

impl CallTimer {
    pub fn disable(&mut self) {
        self.interval = 0;
        self.inner.cancel().unwrap();
    }

    pub fn is_active(&mut self) -> bool {
        self.inner.is_scheduled().unwrap()
    }

    pub fn get_interval(&mut self) -> i64 {
        self.interval
    }

    pub fn tick_every(&mut self, interval: i64) {
        self.interval = interval;
        self.inner
            .every(Duration::from_millis(interval as u64))
            .unwrap();
    }
}

impl CustomType for CallTimer {
    fn build(mut builder: rhai::TypeBuilder<Self>) {
        builder
            .with_name("CallTimer")
            .with_get("interval", Self::get_interval)
            .with_fn("is_active", Self::is_active)
            .with_fn("tick_every", Self::tick_every)
            .with_fn("disable", Self::disable);
    }
}
