#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use defmt_rtt as _;
use log::info;

use static_cell::StaticCell;

use embassy_executor::Spawner;
use embassy_net::{Stack, StackResources};
use embassy_time::{Duration, Timer};

use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;

use watering_system::home_assistant::{HomeAssistantFacade, HomeAssistantFacadeConfig};
use watering_system::mdns::MdnsFacade;
use watering_system::mqtt::{MqttFacade, MqttFacadeConfig};
use watering_system::pump::PumpFacade;
use watering_system::sensors::{SensorsFacade, SensorsValues};
use watering_system::wifi::{WiFiFacade, WiFiFacadeConfig};

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

static WIFI_INIT: StaticCell<esp_wifi::EspWifiController> = StaticCell::new();
static RESOURCES: StaticCell<StackResources<5>> = StaticCell::new();
static NET_STACK: StaticCell<Stack<'static>> = StaticCell::new();

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // generator version: 0.5.0

    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timer0 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timer0.timer0);

    info!("Embassy initialized!");
    let rng = esp_hal::rng::Rng::new(peripherals.RNG);
    let timer1 = TimerGroup::new(peripherals.TIMG0);
    let wifi_init = WIFI_INIT.init(
        esp_wifi::init(timer1.timer0, rng).expect("Failed to initialize WIFI/BLE controller"),
    );
    let (mut _wifi_controller, _interfaces) = esp_wifi::wifi::new(wifi_init, peripherals.WIFI)
        .expect("Failed to initialize WIFI controller");
    let stack_resources = RESOURCES.init(StackResources::<5>::new());
    let (mut wifi_facade, stack_tmp, _runner) = WiFiFacade::new(
        WiFiFacadeConfig::from_env(),
        _wifi_controller,
        _interfaces,
        stack_resources,
    );
    let stack: &'static mut Stack<'static> = NET_STACK.init(stack_tmp);
    info!(
        "Free: {}, Used: {}",
        esp_alloc::HEAP.free(),
        esp_alloc::HEAP.used()
    );

    let mdns = MdnsFacade::new();

    info!("Wifi and MQTT facades initialized. Connecting to Wifi..");
    wifi_facade
        .connect()
        .await
        .expect("Failed to connect to WiFi");
    spawner.spawn(net_task(_runner)).unwrap();

    info!("Wifi connected! Fetching broker using mDNS...");
    let (ip, port) = mdns.query_service(env!("MQTT_SERVICE"), stack).await;
    info!("Got IP: {} and Port: {}", ip, port);

    let home_assistant_config = HomeAssistantFacadeConfig::new_from_env();
    let home_assistant: HomeAssistantFacade = HomeAssistantFacade::new(home_assistant_config);
    let pump_topic = home_assistant.get_pump_topic();
    let mqtt_facade_config = MqttFacadeConfig::new(ip, port, "MyDevice", &pump_topic);
    spawner
        .spawn(mqtt_publisher_task(mqtt_facade_config.clone(), stack))
        .unwrap();
    spawner
        .spawn(mqtt_receiver_task(mqtt_facade_config.clone(), stack))
        .unwrap();

    info!("IP Fetched! MQTT worker started..");

    let sensors_facade: SensorsFacade =
        SensorsFacade::new(peripherals.GPIO35, peripherals.ADC1, peripherals.GPIO33);
    let pump_facade: PumpFacade = PumpFacade::new(peripherals.GPIO27);

    spawner
        .spawn(sensors_loop(
            sensors_facade,
            home_assistant_config,
            mqtt_facade_config.clone(),
        ))
        .unwrap();
    spawner
        .spawn(pump_loop(
            pump_facade,
            home_assistant_config,
            mqtt_facade_config.clone(),
        ))
        .unwrap();

    // Keep the main function alive
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

#[embassy_executor::task]
async fn net_task(
    mut runner: embassy_net::Runner<'static, esp_wifi::wifi::WifiDevice<'static>>,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn mqtt_publisher_task(
    mqtt_facade_config: MqttFacadeConfig,
    stack: &'static Stack<'static>,
) -> ! {
    MqttFacade::new(mqtt_facade_config)
        .run_publisher_worker(stack)
        .await
}

#[embassy_executor::task]
async fn mqtt_receiver_task(
    mqtt_facade_config: MqttFacadeConfig,
    stack: &'static Stack<'static>,
) -> ! {
    MqttFacade::new(mqtt_facade_config)
        .run_receiver_worker(stack)
        .await
}

#[embassy_executor::task]
async fn sensors_loop(
    mut sensors_facade: SensorsFacade<'static>,
    home_assistant_config: HomeAssistantFacadeConfig,
    mqtt_facade_config: MqttFacadeConfig,
) -> ! {
    let home_assistant: HomeAssistantFacade = HomeAssistantFacade::new(home_assistant_config);
    let mut mqtt_facade = MqttFacade::new(mqtt_facade_config);
    
    // Send discovery messages
    mqtt_facade.send_message(home_assistant.get_discovery_message_temperature().unwrap());
    mqtt_facade.send_message(home_assistant.get_discovery_message_humidity().unwrap());
    mqtt_facade.send_message(
        home_assistant
            .get_discovery_message_soil_moisture()
            .unwrap(),
    );
    mqtt_facade.send_message(home_assistant.get_discovery_message_pump().unwrap());

    loop {
        let sensors_values: SensorsValues = sensors_facade.read_values().await;
        info!(
            "Sensors values: {:?}, {:?}, {:?}",
            sensors_values.soil_moisture_sensor_value,
            sensors_values.temperature,
            sensors_values.humidity
        );

        let message = home_assistant.get_sensors_state_mqtt_message(sensors_values);
        mqtt_facade.send_message(message.unwrap());

        Timer::after(Duration::from_secs(10)).await;
    }
}


#[embassy_executor::task]
async fn pump_loop(
    mut pump_facade: PumpFacade<'static>,
    home_assistant_config: HomeAssistantFacadeConfig,
    mqtt_facade_config: MqttFacadeConfig,
) -> ! {
    let home_assistant: HomeAssistantFacade = HomeAssistantFacade::new(home_assistant_config);
    let mut mqtt_facade = MqttFacade::new(mqtt_facade_config);
    
    // Send discovery messages
    mqtt_facade.send_message(home_assistant.get_discovery_message_pump().unwrap());
    pump_facade.turn_off();

    loop {
        match mqtt_facade.poll_message() {
            Some(message) => {
                info!("Received message: {:?}", message.content);
                if pump_facade.is_on() == true {
                    info!("Pump is on, turning off..");
                    pump_facade.turn_off();
                } else {
                    info!("Pump is off, turning on..");
                    pump_facade.turn_on();
                }

                let message = home_assistant.get_pump_state_mqtt_message(pump_facade.is_on());
                mqtt_facade.send_message(message.unwrap());
            }
            None => {
                info!("No message received");
            }
        }

        Timer::after(Duration::from_millis(2000)).await;
    }
}


