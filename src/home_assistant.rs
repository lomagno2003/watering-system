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

    pub fn get_state_mqtt_message(
        &self, 
        sensors_values: SensorsValues
    ) -> Option<MqttMessage> {
        let mut topic_buffer: String<128> = String::new();
        let mut message_buffer: String<256> = String::new();

        write!(&mut topic_buffer, "homeassistant/device/{}/state", self._config.device_id).ok()?;
        write!(&mut message_buffer,
            r#"{{"temperature":{},"humidity":{},"soil_moisture":{}}}"#,
            sensors_values.temperature,
            sensors_values.humidity,
            sensors_values.soil_moisture_sensor_value
        ).ok()?;

        MqttMessage::new(
            topic_buffer.as_str(),
            message_buffer.as_str()
        )
    }

    pub fn get_device_discovery_topic_and_content(&self) -> Option<MqttMessage> {
        let mut topic_buffer: String<128> = String::new();
        let mut message_buffer: String<256> = String::new();
        
        write!(&mut topic_buffer, "homeassistant/device/{}/config", self._config.device_id).ok()?;
        // Simplified discovery message to fit in smaller buffer
        write!(&mut message_buffer, 
            r#"{{"dev":{{"ids":"{}","name":"WateringSystem"}},"state_topic":"homeassistant/device/{}/state"}}"#,
            self._config.device_id, self._config.device_id
        ).ok()?;

        MqttMessage::new(
            topic_buffer.as_str(),
            message_buffer.as_str()
        )
    }
}
