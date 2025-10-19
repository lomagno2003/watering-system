use log::{error, info};
use esp_wifi::wifi::{
    Interfaces,
    ClientConfiguration, 
    Configuration, 
    WifiController,
};
use embassy_time::Timer;
use embassy_net::{Config, Stack, StackResources, Runner};
use esp_wifi::wifi::WifiDevice;

#[derive(Debug)]
pub enum WiFiError {
    ConnectionFailed,
    ConfigurationError,
    NetworkError,
    InitializationFailed,
    DhcpFailed,
}
use esp_wifi::wifi::sta_state;

pub struct WiFiFacadeConfig {
    pub ssid: &'static str,
    pub password: &'static str,
}

impl WiFiFacadeConfig {
    pub fn new(wifi_ssid: &'static str, wifi_password: &'static str) -> Self {
        Self {
            ssid: wifi_ssid,
            password: wifi_password,
        }
    }

    pub fn from_env() -> Self {
        Self {
            ssid: env!("WIFI_SSID"),
            password: env!("WIFI_PASSWORD"),
        }
    }
}

pub struct WiFiFacade<'lifetime> {
    _config: WiFiFacadeConfig,
    _wifi_controller: WifiController<'lifetime>,
}


impl <'lifetime> WiFiFacade<'lifetime> {
    pub fn new(
        config: WiFiFacadeConfig, 
        wifi_controller: WifiController<'lifetime>, 
        interfaces: Interfaces<'lifetime>,
        stack_resources: &'lifetime mut StackResources<5>,
    ) -> (Self, Stack<'lifetime>, Runner<'lifetime, WifiDevice<'lifetime>>) {
        let facade = Self {
            _config: config,
            _wifi_controller: wifi_controller,
        };

        let dhcpconfig = Config::dhcpv4(Default::default());
        let (stack, runner) = embassy_net::new(
            interfaces.sta, 
            dhcpconfig, 
            stack_resources, 
            3845834);
            
        return (facade, stack, runner)
    }

    pub async fn connect(&mut self) -> Result<(), WiFiError> {
        self.configure().unwrap();
        self.connect_to_wifi().await.unwrap();
        
        Ok(())
    }

    fn configure(&mut self) -> Result<(), WiFiError> {
        self._wifi_controller.set_configuration(&Configuration::Client(ClientConfiguration {
            ssid: self._config.ssid.try_into().map_err(|e| {
                error!("❌ Failed to convert SSID to WiFi format: {:?}", e);
                WiFiError::ConfigurationError
            })?,
            password: self._config.password.try_into().map_err(|e| {
                error!("❌ Failed to convert password to WiFi format: {:?}", e);
                WiFiError::ConfigurationError
            })?,
            ..Default::default()
        })).map_err(|e| {
            error!("❌ Failed to set WiFi configuration: {:?}", e);
            WiFiError::ConfigurationError
        })?;

        Ok(())
    }

    async fn connect_to_wifi(&mut self) -> Result<(), WiFiError> {
        self._wifi_controller.start().map_err(|e| {
            error!("❌ Failed to start WiFi: {:?}", e);
            WiFiError::InitializationFailed
        })?;
        self._wifi_controller.connect().map_err(|e| {
            error!("❌ Failed to connect to WiFi: {:?}", e);
            WiFiError::ConnectionFailed
        })?;

        match self._wifi_controller.scan_n(10) {
            Ok(aps) => {
                for ap in aps {
                    info!("{:?}", ap);
                }
            }
            Err(err) => info!("Scan error: {:?}", err)
        };

        loop {
            match self._wifi_controller.is_connected() {
                Ok(true) => {
                    let state = sta_state();
                    info!("Wifi Connected: {:?}", state);
                    
                    break;
                },
                Ok(false) => {
                    info!("Wifi Connecting..");
                    Timer::after_millis(500).await;
                }
                Err(err) => {
                    error!("Error when connecting to Wifi: {:?}", err);
                    Timer::after_millis(500).await;
                },
            }
        }
    
        Ok(())
    }
}