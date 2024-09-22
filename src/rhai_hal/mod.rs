pub mod health;
pub mod ledc;
pub mod timer;
pub mod ws2812;

pub fn register(engine: &mut rhai::Engine) {
    engine.build_type::<health::SystemHealth>();
    engine.build_type::<ledc::PwmController>();
    engine.build_type::<timer::CallTimer>();
    engine.build_type::<ws2812::RgbLed>();
}
