fn init() {
    this.counter = 0;
    // this.timer.tick_every(500);
    this.last_time = timestamp();
}

fn set_percent(new_pct) {
    print("old percent");
    print(this.pwm.duty_percent);
    this.pwm.set_duty_percent(new_pct);
}

fn tick(tick_time) {
    print(this.last_time);
    print(tick_time);
    print(tick_time - this.last_time);
    this.last_time = tick_time;
}

fn lovense(args) {
    print(args[0]);
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