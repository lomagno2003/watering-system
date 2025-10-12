use embassy_time::{Duration, Timer};
use esp_hal::analog::adc::{Adc, AdcConfig, AdcPin, Attenuation};
use esp_hal::peripherals::{ADC1, GPIO35};
use esp_hal::Blocking;

use log::info;

pub struct SensorsValues {
    pub soil_moisture_sensor_value: u16,
}

impl SensorsValues {
    pub fn new(soil_moisture_sensor_value: u16) -> Self {
        SensorsValues {
            soil_moisture_sensor_value,
        }
    }
}

pub struct SensorsFacade<'lifetime> {
    _soil_moisture_sensor_adc: Adc<'lifetime, ADC1<'static>, Blocking>,
    _soil_moisture_sensor_adc_pin: AdcPin<GPIO35<'static>, ADC1<'static>>,
}

impl<'lifetime> SensorsFacade<'lifetime> {
    pub fn new(gpio: GPIO35<'static>, adc: ADC1<'static>) -> Self {
        let mut soil_moisture_sensor_adc_config = AdcConfig::new();
        let soil_moisture_sensor_adc_pin =
            soil_moisture_sensor_adc_config.enable_pin(gpio, Attenuation::_11dB);
        let soil_moisture_sensor_adc: Adc<'lifetime, ADC1<'static>, Blocking> =
            Adc::new(adc, soil_moisture_sensor_adc_config);

        SensorsFacade {
            _soil_moisture_sensor_adc: soil_moisture_sensor_adc,
            _soil_moisture_sensor_adc_pin: soil_moisture_sensor_adc_pin,
        }
    }

    pub async fn read_values(&mut self) -> SensorsValues {
        let soil_moisture_sensor_value: u16;
        loop {
            info!("Sensors: Reading value");
            match self._soil_moisture_sensor_adc.read_oneshot(&mut self._soil_moisture_sensor_adc_pin) {
                Ok(v) => {
                    info!("Sensors: Value: {}", v);
                    soil_moisture_sensor_value = v;
                    break;
                }
                Err(e) => {
                    info!("Sensors: ADC read error: {:?}", e);
                }
            }
            Timer::after(Duration::from_secs(1)).await;
        }

        return SensorsValues::new(soil_moisture_sensor_value);
    }
}
