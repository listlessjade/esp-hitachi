fn init() {
    this.counter = 0;
    this.timer.tick_every(3000);
    this.last_time = timestamp();
}

fn set_percent(new_pct) {
    print("old percent");
    print(this.pwm.duty_percent);
    this.pwm.set_duty_percent(new_pct);
}

fn tick(tick_time) {
    print(`time since last tick: ${tick_time - this.last_time}ms`);
    print(`chip temp: ${this.system.chip_temperature()} C`);
    print(`free memory: ${this.system.free_memory().to_float() / 1000}kb`);
    this.last_time = tick_time;
    this.led.set_color(rand(0..255), rand(0..255), rand(0..255));
}

fn disable_timer() {
    this.timer.disable();
}

fn led_color(r, g, b) {
    this.led.set_color(parse_int(r), parse_int(g), parse_int(b));
}

fn lovense(args) {
    print(`lovense cmd: ${args}`);
    // print(args[0]);
    switch args[0] {
        "DeviceType" => {
            return [];
            // return ["T", "11", ""];
        },
        "Battery" => {
            return ["100"];
        },
        "Status" => {
            return ["2"];
        },
        "GetLight" => {
            return ["Light", "1"];
        },
        "Vibrate" => {            
            const lovense_range = 0..20;
            const target_range = 50..100;

            let strength = parse_int(args[1]);

            let mapped_strength = if strength > 0 { map_range(lovense_range, target_range, strength) } else { 0 };

            print(`setting duty to ${mapped_strength}% (from ${strength}/${lovense_range.end})`);
            this.pwm.set_duty_percent(mapped_strength);
        },
        _ => {
            return [];
        }
    }

    return [];
}

fn map_range(lhs, rhs, val) {
    return rhs.start + ((val - lhs.start) * (rhs.end - rhs.start) / (lhs.end - lhs.start))
}