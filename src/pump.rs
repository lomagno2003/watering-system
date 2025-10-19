use esp_hal::gpio::{Flex};
use esp_hal::peripherals::{GPIO27};


pub struct PumpFacade<'lifetime> {
    _pump_gpio: Flex<'lifetime>,
    _is_on: bool
}

impl <'lifetime> PumpFacade<'lifetime> {
    pub fn new(
        pump_pin: GPIO27<'static>,
    ) -> Self {
        let mut pump_gpio = Flex::new(pump_pin);
        pump_gpio.set_input_enable(true);
        pump_gpio.set_output_enable(true);

        PumpFacade {
            _pump_gpio: pump_gpio,
            _is_on: false
        }
    }

    pub fn turn_on(&mut self) {
        self._pump_gpio.set_low();
        self._is_on = true;
    }

    pub fn turn_off(&mut self) {
        self._pump_gpio.set_high();
        self._is_on = false;
    }

    pub fn is_on(&self) -> bool {
        self._is_on
    }
}