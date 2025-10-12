use esp_hal::gpio::{Flex};
use esp_hal::peripherals::{GPIO26};


pub struct PumpFacade<'lifetime> {
    _pump_gpio: Flex<'lifetime>
}

impl <'lifetime> PumpFacade<'lifetime> {
    pub fn new(
        pump_pin: GPIO26<'static>,
    ) -> Self {
        let mut pump_gpio = Flex::new(pump_pin);
        pump_gpio.set_input_enable(true);
        pump_gpio.set_output_enable(true);

        PumpFacade {
            _pump_gpio: pump_gpio
        }
    }

    pub fn turn_on(&mut self) {
        self._pump_gpio.set_low();
    }

    pub fn turn_off(&mut self) {
        self._pump_gpio.set_high();
    }
}