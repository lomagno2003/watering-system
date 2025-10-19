use embassy_time::{Duration, Timer};

use esp_hal::analog::adc::{Adc, AdcConfig, AdcPin, Attenuation};
use esp_hal::peripherals::{GPIO33, GPIO35, ADC1};
use esp_hal::gpio::{Flex, InputConfig, OutputConfig, DriveMode, Pull};
use esp_hal::Blocking;
use esp_hal::delay::Delay;

use log::{info, warn};

use embedded_dht_rs::dht22::Dht22;


pub struct SensorsValues {
    pub soil_moisture_sensor_value: u16,
    pub temperature: f32,
    pub humidity: f32,
}

impl SensorsValues {
    pub fn new(
        soil_moisture_sensor_value: u16,
        temperature: f32,
        humidity: f32,
    ) -> Self {
        SensorsValues {
            soil_moisture_sensor_value,
            temperature,
            humidity,
        }
    }
}

pub struct SensorsFacade<'lifetime> {
    _soil_moisture_sensor_adc: Adc<'lifetime, ADC1<'static>, Blocking>,
    _soil_moisture_sensor_adc_pin: AdcPin<GPIO35<'static>, ADC1<'static>>,
    _dht22_sensor: Dht22<Flex<'lifetime>, Delay>,
}

impl<'lifetime> SensorsFacade<'lifetime> {
    pub fn new(
        soil_moisture_sensor_pin_peripheral: GPIO35<'static>,
        soil_moisture_sensor_adc_peripheral: ADC1<'static>,
        dht_pin_peripheral: GPIO33<'static>,
    ) -> Self {
        // Initialize soil moisture sensor
        let mut soil_moisture_sensor_adc_config = AdcConfig::new();
        let soil_moisture_sensor_adc_pin =
            soil_moisture_sensor_adc_config.enable_pin(soil_moisture_sensor_pin_peripheral, Attenuation::_11dB);
        let soil_moisture_sensor_adc: Adc<'lifetime, ADC1<'static>, Blocking> =
            Adc::new(soil_moisture_sensor_adc_peripheral, soil_moisture_sensor_adc_config);

        // Initialize DHT22 sensor
        let mut dht22_pin = Flex::new(dht_pin_peripheral);
        dht22_pin.apply_output_config(
            &OutputConfig::default().with_drive_mode(DriveMode::OpenDrain)
        );

        // --- Input config: enable pull-up (line idles high when released) ---
        dht22_pin.apply_input_config(
            &InputConfig::default().with_pull(Pull::Up)
        );

        // Start released (HIGH, pulled up)
        dht22_pin.set_high();
        dht22_pin.set_input_enable(true);
        dht22_pin.set_output_enable(true);
        let delay: Delay = Delay::new();
        let dht22_sensor: Dht22<Flex<'_>, Delay> = Dht22::new(dht22_pin, delay);

        SensorsFacade {
            _soil_moisture_sensor_adc: soil_moisture_sensor_adc,
            _soil_moisture_sensor_adc_pin: soil_moisture_sensor_adc_pin,
            _dht22_sensor: dht22_sensor,
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
                    warn!("Sensors: Soil Moisture read error: {:?}", e);
                }
            }
            Timer::after(Duration::from_millis(100)).await;
        }

        let temperature: f32;
        let humidity: f32;

        loop {
            match self._dht22_sensor.read() {
                Ok(reading) => {
                    info!("Sensors: Temperature: {}°C, Humidity: {}%", reading.temperature, reading.humidity);
                    temperature = reading.temperature;
                    humidity = reading.humidity;
                    break;
                }
                Err(e) => {
                    warn!("Sensors: DHT22 read error: {:?}", e);
                }
            }
            Timer::after(Duration::from_millis(100)).await;
        }
        
        return SensorsValues::new(
            soil_moisture_sensor_value,
            temperature,
            humidity,
        );
    }
}
