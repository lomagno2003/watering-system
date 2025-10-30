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
        sensors_values: SensorsValues,
        pump_on: bool
    ) -> Option<MqttMessage> {
        let mut topic_buffer: String<128> = String::new();
        let mut message_buffer: String<256> = String::new();

        write!(&mut topic_buffer, "homeassistant/device/{}/state", self._config.device_id).ok()?;
        write!(&mut message_buffer,
            r#"{{"temperature":{},"humidity":{},"soil_moisture":{},"pump_state":"{}"}}"#,
            sensors_values.temperature,
            sensors_values.humidity,
            sensors_values.soil_moisture_sensor_value,
            if pump_on {"ON"} else {"OFF"}
        ).ok()?;

        MqttMessage::new(
            topic_buffer.as_str(),
            message_buffer.as_str()
        )
    }

    pub fn get_discovery_message_temperature(&self) -> Option<MqttMessage> {
        let mut topic_buffer: String<128> = String::new();
        let mut message_buffer: String<2048> = String::new();
        
        write!(&mut topic_buffer, "homeassistant/device/{}/config", self._config.device_id).ok()?;
        write!(&mut message_buffer,
r#"{{
"dev":{{"ids":"{id}","name":"WateringSystem"}},
"o": {{"name":"watering-system"}},
"cmps":{{"temperature_cmp":{{"p":"sensor","dev_cla":"temperature","unit_of_measurement":"°C","val_tpl":"{{{{ value_json.temperature }}}}","unique_id":"{id}-temperature"}}}},
"state_topic":"homeassistant/device/{id}/state"
}}"#,
            id = self._config.device_id
        ).unwrap();

        MqttMessage::new(
            topic_buffer.as_str(),
            message_buffer.as_str()
        )
    }

    pub fn get_discovery_message_humidity(&self) -> Option<MqttMessage> {
        let mut topic_buffer: String<128> = String::new();
        let mut message_buffer: String<2048> = String::new();
        
        write!(&mut topic_buffer, "homeassistant/device/{}/config", self._config.device_id).ok()?;
        write!(&mut message_buffer,
r#"{{
"dev":{{"ids":"{id}","name":"WateringSystem"}},
"o": {{"name":"watering-system"}},
"cmps":{{"humidity_cmp":{{"p":"sensor","dev_cla":"humidity","unit_of_measurement":"%","val_tpl":"{{{{ value_json.humidity }}}}","unique_id":"{id}_humidity"}}}},
"state_topic":"homeassistant/device/{id}/state"
}}"#,
            id = self._config.device_id
        ).unwrap();

        MqttMessage::new(
            topic_buffer.as_str(),
            message_buffer.as_str()
        )
    }


    pub fn get_discovery_message_soil_moisture(&self) -> Option<MqttMessage> {
        let mut topic_buffer: String<128> = String::new();
        let mut message_buffer: String<2048> = String::new();
        
        write!(&mut topic_buffer, "homeassistant/device/{}/config", self._config.device_id).ok()?;
        write!(&mut message_buffer,
r#"{{
"dev":{{"ids":"{id}","name":"WateringSystem"}},
"o": {{"name":"watering-system"}},
"cmps":{{"soil_cmp":{{"p":"sensor","name":"Soil moisture","unit_of_measurement":"%","dev_cla":"moisture","val_tpl":"{{{{ value_json.soil_moisture }}}}","unique_id":"{id}_soil"}}}},
"state_topic":"homeassistant/device/{id}/state"
}}"#,
            id = self._config.device_id
        ).unwrap();

        MqttMessage::new(
            topic_buffer.as_str(),
            message_buffer.as_str()
        )
    }

    pub fn get_discovery_message_pump(&self) -> Option<MqttMessage> {
        let mut topic_buffer: String<128> = String::new();
        let mut message_buffer: String<2048> = String::new();
        
        write!(&mut topic_buffer, "homeassistant/device/{}/config", self._config.device_id).ok()?;
        write!(&mut message_buffer,
r#"{{
"dev":{{"ids":"{id}","name":"WateringSystem"}},
"o": {{"name":"watering-system"}},
"cmps":{{"pump_cmp":{{"p":"switch","name":"Pump","command_topic":"{topic}","val_tpl":"{{{{ value_json.pump_state }}}}","unique_id":"{id}_pump"}}}},
"state_topic":"homeassistant/device/{id}/state"
}}"#,
            id = self._config.device_id,
            topic = self.get_pump_topic().as_str()
        ).unwrap();

        MqttMessage::new(
            topic_buffer.as_str(),
            message_buffer.as_str()
        )
    }

    pub fn get_pump_topic(&self) -> String<128> {
        let mut topic_buffer: String<128> = String::new();
        write!(&mut topic_buffer, "homeassistant/device/{}/pump", self._config.device_id).ok();
        topic_buffer
    }
}