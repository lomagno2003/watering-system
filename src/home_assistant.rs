use crate::mqtt::MqttMessage;
use crate::sensors::SensorsValues;

pub struct HomeAssistantFacadeConfig {
    device_id: &'static str
}

impl HomeAssistantFacadeConfig {
    pub fn new(device_id: &'static str) -> Self {
        Self {
            device_id: device_id
        }
    }

    pub fn new_from_env() -> Self {
        Self {
            device_id: env!("DEVICE_NAME")
        }
    }
}

pub struct HomeAssistantFacade {
    _config: HomeAssistantFacadeConfig,
}

use core::fmt::Write;
use heapless::String;

impl HomeAssistantFacade {
    pub fn new(config: HomeAssistantFacadeConfig) -> Self {
        Self {
            _config: config,
        }
    }

    pub fn get_state_mqtt_message<'m>(
        &self, 
        sensors_values: SensorsValues
    ) -> MqttMessage<'m> {
        unsafe {
            static mut topic_buffer: String<128> = String::new();
            static mut message_buffer: String<1024> = String::new();

            topic_buffer.clear();
            message_buffer.clear();

            write!(&mut topic_buffer, "homeassistant/device/{}/state", self._config.device_id).unwrap();
            write!(&mut message_buffer,
                r#"{{"temperature":{},"humidity":{},"soil_moisture":{}}}"#,
                sensors_values.temperature,
                sensors_values.humidity,
                sensors_values.soil_moisture_sensor_value
            ).unwrap();

            return MqttMessage::new(
                topic_buffer.as_str(),
                message_buffer.as_str()
            );
        }
    }

    pub fn get_device_discovery_mqtt_message<'m>(&self) -> MqttMessage<'m> {
        unsafe {
            static mut topic_buffer: String<128> = String::new();
            static mut message_buffer: String<4096> = String::new();

            topic_buffer.clear();
            message_buffer.clear();
            
            write!(&mut topic_buffer, "homeassistant/device/{}/config", self._config.device_id).unwrap();
            write!(&mut message_buffer, 
                r#"{{
                    "dev": {{
                        "ids": "{}",
                        "name": "WateringSystem"
                    }},
                    "o": {{
                        "name":"watering-system",
                        "sw": "1.0",
                        "url": "https://github.com/lomagno2003/watering-system"
                    }},
                    "cmps": {{
                        "temperature_component": {{
                            "p": "sensor",
                            "device_class":"temperature",
                            "unit_of_measurement":"°C",
                            "value_template":"{{{{ value_json.temperature}}}}",
                            "unique_id":"{}_temperature"
                        }},
                        "humidity_component": {{
                            "p": "sensor",
                            "device_class":"humidity",
                            "unit_of_measurement":"%",
                            "value_template":"{{{{ value_json.humidity}}}}",
                            "unique_id":"{}_humidity"
                        }},
                        "soil_moisture_component": {{
                            "p": "sensor",
                            "device_class":"moisture",
                            "unit_of_measurement":"%",
                            "value_template":"{{{{ value_json.soil_moisture}}}}",
                            "unique_id":"{}_soil_moisture"
                        }}
                    }},
                    "state_topic":"homeassistant/device/{}/state",
                    "qos": 2
                }}"#,
                self._config.device_id, self._config.device_id, self._config.device_id, self._config.device_id, self._config.device_id).unwrap();

            return MqttMessage::new(
                topic_buffer.as_str(), 
                message_buffer.as_str()
            );
        }
    }
}
