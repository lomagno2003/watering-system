use core::net::IpAddr;
use core::net::SocketAddr;
use embassy_net::{
    tcp::client::{TcpClient, TcpClientState},
    Stack,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::Timer;
use embedded_nal_async::TcpConnect;
use log::{info,warn,error};
use rust_mqtt::packet::v5::publish_packet::QualityOfService;
use rust_mqtt::{
    client::{
        client::MqttClient,
        client_config::{ClientConfig, MqttVersion},
    },
    utils::rng_generator::CountingRng,
};
use static_cell::StaticCell;

#[derive(Clone)]
pub struct MqttFacadeConfig {
    pub broker_ip: IpAddr,
    pub broker_port: u16,
    pub client_id: &'static str,
    pub topic_id: String<MAX_TOPIC>,
}

impl MqttFacadeConfig {
    pub fn new(broker_ip: IpAddr, broker_port: u16, client_id: &'static str, topic_id: &str) -> Self {
        let mut topic = String::new();
        topic.push_str(topic_id).expect("Topic too long");
        
        Self {
            broker_ip,
            broker_port,
            client_id,
            topic_id: topic,
        }
    }
}

use heapless::String;

pub struct MqttMessage {
    pub topic: String<MAX_TOPIC>,
    pub content: String<MAX_PAYLOAD>,
}

impl MqttMessage {
    pub fn new(mqtt_topic: &str, mqtt_message_content: &str) -> Option<Self> {
        let mut topic = String::new();
        let mut content = String::new();

        if topic.push_str(mqtt_topic).is_err() || content.push_str(mqtt_message_content).is_err() {
            return None;
        }

        Some(Self { topic, content })
    }
}
const IN_CAP: usize = 5;
const OUT_CAP: usize = 5;
const MAX_TOPIC: usize = 64;
const MAX_PAYLOAD: usize = 512;

const MQTT_SEND_BUFFER_SIZE: usize = 2048;
const MQTT_RECV_BUFFER_SIZE: usize = 2048;
const TCP_SEND_BUFFER_SIZE: usize = 2048;
const TCP_RECV_BUFFER_SIZE: usize = 2048;
const QUALITY_OF_SERVICE: QualityOfService = QualityOfService::QoS1;

static INBOUND: Channel<CriticalSectionRawMutex, MqttMessage, IN_CAP> = Channel::new();
static OUTBOUND: Channel<CriticalSectionRawMutex, MqttMessage, OUT_CAP> = Channel::new();

pub struct MqttFacade {
    _config: MqttFacadeConfig,
}

impl MqttFacade {
    pub fn new(config: MqttFacadeConfig) -> Self {
        Self { _config: config }
    }

    pub fn send_message<'s>(&mut self, message: MqttMessage) {
        info!(
            "MqttFacade: Queuing message to host {:?}, port {:?}, topic {:?}, content {:?}",
            self._config.broker_ip, self._config.broker_port, message.topic, message.content
        );

        if OUTBOUND.try_send(message).is_err() {
            warn!("MqttFacade: Message queue full, dropping message");
        }
    }

    pub fn poll_message(&mut self) -> Option<MqttMessage> {
        INBOUND.try_receive().ok()
    }

    pub async fn run_publisher_worker<'s>(&mut self, stack: &'static Stack<'s>) -> ! {
        static SEND_BUFFER: StaticCell<[u8; MQTT_SEND_BUFFER_SIZE]> = StaticCell::new();
        static RECEIVE_BUFFER: StaticCell<[u8; MQTT_RECV_BUFFER_SIZE]> = StaticCell::new();

        let send_buffer = SEND_BUFFER.init([0_u8; MQTT_SEND_BUFFER_SIZE]);
        let receive_buffer = RECEIVE_BUFFER.init([0_u8; MQTT_RECV_BUFFER_SIZE]);

        loop {
            if OUTBOUND.is_empty() {
                info!("MqttWorker - Publisher: No messages to send. Waiting...");
                Timer::after_millis(500).await;
                continue;
            }

            if !stack.is_link_up() {
                info!("MqttWorker - Publisher: Network is down. Waiting..");
                Timer::after_millis(500).await;
                continue;
            } else {
                info!("MqttWorker - Publisher: Network is up!");
            }

            if stack.config_v4().is_none() {
                info!("MqttWorker - Publisher: DHCP not configured yet. Waiting..");
                Timer::after_millis(500).await;
                continue;
            } else {
                info!("MqttWorker - Publisher: DHCP configured!");
                Timer::after_millis(100).await;
            }

            info!("MqttWorker - Publisher: Creating TCP client state...");
            let state: TcpClientState<3, TCP_SEND_BUFFER_SIZE, TCP_RECV_BUFFER_SIZE> =
                TcpClientState::new();
            info!("MqttWorker - Publisher: TCP client state created");

            let tcp_client = TcpClient::new(*stack, &state);
            info!("MqttWorker - Publisher: TCP client created, attempting connection to {} and port {}", self._config.broker_ip, self._config.broker_port,);

            let tcp_connection = match tcp_client
                .connect(SocketAddr::new(
                    self._config.broker_ip,
                    self._config.broker_port,
                ))
                .await
            {
                Ok(tcp_connection) => {
                    info!("MqttWorker - Publisher: TCP connection established successfully");
                    tcp_connection
                }
                Err(e) => {
                    info!("MqttWorker - Publisher: TCP connection failed: {:?}", e);
                    Timer::after_millis(500).await;
                    continue;
                }
            };

            info!("MqttWorker - Publisher: Creating MQTT client...");
            send_buffer.fill(0);
            receive_buffer.fill(0);

            let mqtt_client_config: ClientConfig<'_, 5, CountingRng> =
                ClientConfig::new(MqttVersion::MQTTv5, CountingRng(12345));
            let mut mqtt_client = MqttClient::new(
                tcp_connection,
                send_buffer,
                MQTT_SEND_BUFFER_SIZE,
                receive_buffer,
                MQTT_RECV_BUFFER_SIZE,
                mqtt_client_config,
            );

            info!("MqttWorker - Publisher: MQTT client created, attempting broker connection...");
            match mqtt_client.connect_to_broker().await {
                Ok(_) => {
                    info!("MqttWorker - Publisher: MQTT broker connection established");
                }
                Err(e) => {
                    info!("MqttWorker - Publisher: MQTT broker connection failed: {:?}", e);
                    Timer::after_millis(500).await;
                    continue;
                }
            };

            let message = OUTBOUND.receive().await;
            info!("MqttWorker - Publisher: Attempting to send message (topic: {} bytes, content: {} bytes)...", 
                    message.topic.len(), message.content.len());
            info!("MqttWorker - Publisher: Attempting to send message (topic: {}, content: {})", 
                    message.topic.as_str(), message.content);

            match mqtt_client
                .send_message(
                    message.topic.as_str(),
                    message.content.as_bytes(),
                    QUALITY_OF_SERVICE,
                    false,
                ).await {
                Ok(_) => {
                    info!("MqttWorker - Publisher: Message sent successfully");
                }
                Err(e) => {
                    error!("MqttWorker - Publisher: Error when sending message: {}", e);
                    Timer::after_millis(500).await;
                }
            };
        }
    }

    pub async fn run_receiver_worker<'s>(&mut self, stack: &'static Stack<'s>) -> ! {
        static SEND_BUFFER: StaticCell<[u8; MQTT_SEND_BUFFER_SIZE]> = StaticCell::new();
        static RECEIVE_BUFFER: StaticCell<[u8; MQTT_RECV_BUFFER_SIZE]> = StaticCell::new();

        let send_buffer = SEND_BUFFER.init([0_u8; MQTT_SEND_BUFFER_SIZE]);
        let receive_buffer = RECEIVE_BUFFER.init([0_u8; MQTT_RECV_BUFFER_SIZE]);

        loop {
            if !stack.is_link_up() {
                info!("MqttWorker - Receiver: Network is down. Waiting..");
                Timer::after_millis(500).await;
                continue;
            } else {
                info!("MqttWorker - Receiver: Network is up!");
            }

            if stack.config_v4().is_none() {
                info!("MqttWorker - Receiver: DHCP not configured yet. Waiting..");
                Timer::after_millis(500).await;
                continue;
            } else {
                info!("MqttWorker - Receiver: DHCP configured!");
                Timer::after_millis(100).await;
            }

            info!("MqttWorker - Receiver: Creating TCP client state...");
            let state: TcpClientState<3, TCP_SEND_BUFFER_SIZE, TCP_RECV_BUFFER_SIZE> =
                TcpClientState::new();
            info!("MqttWorker - Receiver: TCP client state created");

            let tcp_client = TcpClient::new(*stack, &state);
            info!("MqttWorker - Receiver: TCP client created, attempting connection...");

            let tcp_connection = match tcp_client
                .connect(SocketAddr::new(
                    self._config.broker_ip,
                    self._config.broker_port,
                ))
                .await
            {
                Ok(tcp_connection) => {
                    info!("MqttWorker - Receiver: TCP connection established successfully");
                    tcp_connection
                }
                Err(e) => {
                    info!("MqttWorker - Receiver: TCP connection failed: {:?}", e);
                    Timer::after_millis(500).await;
                    continue;
                }
            };

            info!("MqttWorker - Receiver: Creating MQTT client...");
            send_buffer.fill(0);
            receive_buffer.fill(0);

            let mqtt_client_config: ClientConfig<'_, 5, CountingRng> =
                ClientConfig::new(MqttVersion::MQTTv5, CountingRng(12345));
            let mut mqtt_client = MqttClient::new(
                tcp_connection,
                send_buffer,
                MQTT_SEND_BUFFER_SIZE,
                receive_buffer,
                MQTT_RECV_BUFFER_SIZE,
                mqtt_client_config,
            );

            info!("MqttWorker - Receiver: MQTT client created, attempting broker connection...");
            match mqtt_client.connect_to_broker().await {
                Ok(_) => {
                    info!("MqttWorker - Receiver: MQTT broker connection established");
                }
                Err(e) => {
                    info!("MqttWorker - Receiver: MQTT broker connection failed: {:?}", e);
                    Timer::after_millis(500).await;
                    continue;
                }
            };

            match mqtt_client.subscribe_to_topic(self._config.topic_id.as_str()).await {
                Ok(_) => {
                    info!("MqttWorker - Receiver: Subscribed to topic {}", self._config.topic_id.as_str());
                }
                Err(e) => {
                    error!("MqttWorker - Receiver: Error when subscribing to topic: {}", e);
                    Timer::after_millis(100).await;
                    continue;
                }
            };

            match mqtt_client.receive_message().await {
                Ok((topic, content)) => {
                    info!("MqttWorker - Receiver: Received message on topic: {:?}", topic);

                    match core::str::from_utf8(content) {
                        Ok(content_str) => {
                            let message = MqttMessage::new(topic, content_str);
                            
                            info!("MqttWorker - Receiver: Received message with content: {:?}", content_str);
                            if let Some(msg) = message {
                                if INBOUND.try_send(msg).is_err() {
                                    warn!("MqttWorker - Receiver: Message queue full, dropping message");
                                }
                            } else {
                                warn!("MqttWorker - Receiver: Message too large, dropping");
                            }
                        }
                        Err(_) => {
                            warn!("MqttWorker - Receiver: Received non-UTF8 message, dropping");
                        }
                    }
                }
                Err(e) => {
                    error!("MqttWorker - Receiver: Error when receiving message: {}", e);
                    Timer::after_millis(100).await;
                    continue;
                }
            };
        }
    }
}