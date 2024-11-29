use embassy_stm32::{gpio::OutputType, peripherals::{PA0, TIM2}, time::hz, timer::{low_level::CountingMode, simple_pwm::{PwmPin, SimplePwm}, Channel}};
use embassy_time::{Duration, Timer};
use firmware_common::driver::buzzer::Buzzer as BuzzerDriver;

pub struct Buzzer {
    pwm: SimplePwm<'static, TIM2>,
}

impl Buzzer {
    pub fn new(timer: TIM2, pin: PA0) -> Self {
        let mut pwm = SimplePwm::new(
            timer,
            Some(PwmPin::new_ch1(pin, OutputType::PushPull)),
            None,
            None,
            None,
            hz(2900),
            CountingMode::EdgeAlignedUp,
        );
        pwm.set_duty(Channel::Ch1, pwm.get_max_duty() / 2);
        pwm.disable(Channel::Ch1);

        Self { pwm }
    }
}

impl BuzzerDriver for Buzzer {
    async fn play(&mut self, frequency: u32, duration_ms: u32) {
        self.pwm.set_frequency(hz(frequency));
        self.pwm.enable(Channel::Ch1);
        Timer::after(Duration::from_millis(duration_ms as u64)).await;
        self.pwm.disable(Channel::Ch1);
    }
}
