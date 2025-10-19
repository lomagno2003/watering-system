// src/mdns.rs
#![allow(dead_code)]

use core::net::{IpAddr, Ipv4Addr};
use embassy_net::{udp, IpAddress, Ipv4Address, Stack};
use embassy_time::{Duration, Instant, Timer};
use esp_hal_mdns::MdnsQuery;
use log::info;

use heapless::String;

const BUFF_SIZE: usize = 4096;

pub struct MdnsFacade;

impl MdnsFacade {
    pub const fn new() -> Self {
        Self
    }

    /// Browse `service_name` and return the first IPv4 address found.
    /// Retries for ~5s total; loop/extend to taste.
    pub async fn query_service<'s>(
        &self,
        service_name: &'static str, // e.g. "_mqtt._tcp.local"
        stack: &'static Stack<'s>,
    ) -> (IpAddr, u16) {
        loop {
            if stack.is_link_up() {
                info!("Network is up.");
                if stack.config_v4().is_some() {
                    info!("DHCP configured!");
                    break;
                }
                info!("DHCP not configured yet. Waiting..");
            } else {
                info!("Network is down. Waiting..");
            }
            Timer::after_millis(400).await;
        }

        let _ = stack
            .join_multicast_group(IpAddress::v4(224, 0, 0, 251))
            .expect("mDNS: join multicast 224.0.0.251 failed");

        let mut rx_meta: [udp::PacketMetadata; 4] = [udp::PacketMetadata::EMPTY; 4];
        let mut rx_buff: [u8; BUFF_SIZE] = [0; BUFF_SIZE];
        let mut tx_meta: [udp::PacketMetadata; 4] = [udp::PacketMetadata::EMPTY; 4];
        let mut tx_buff: [u8; BUFF_SIZE] = [0; BUFF_SIZE];

        let mut sock = udp::UdpSocket::new(
            *stack,
            &mut rx_meta,
            &mut rx_buff,
            &mut tx_meta,
            &mut tx_buff,
        );
        sock.bind(5353)
            .expect("mDNS: bind(5353) failed — is another mDNS/responder running?");
        sock.set_hop_limit(Some(255));

        let mut q = MdnsQuery::new(
            service_name,
            1000, // resend interval ms
            || Instant::now().as_millis() as u64,
        );
        let mdns_peer = (Ipv4Address::new(224, 0, 0, 251), 5353);
        let deadline = Instant::now() + Duration::from_millis(5000);
        let mut rx = [0u8; 1024];

        // Local state variables for caching partial mDNS records
        let mut cached_hostname: Option<heapless::String<64>> = None;
        let mut cached_port: Option<u16> = None;
        let mut cached_ip: Option<[u8; 4]> = None;
        let mut cache_time: Option<u64> = None;

        loop {
            if let Some(pkt) = q.should_send_mdns_packet() {
                info!("mDNS: Querying service {:?}", service_name);
                let _query = sock.send_to(pkt, mdns_peer).await;
                info!("mDNS: Sent query {:?}", _query);
            }
            if let Ok((n, _peer)) = sock.recv_from(&mut rx).await {
                info!("mDNS: Got response: {:?} {:?}", n, _peer);

                let mut ascii: String<1024> = String::new();
                for &b in &rx[..n] {
                    let ch = if b.is_ascii_graphic() || b == b' ' {
                        b as char
                    } else {
                        '.'
                    };
                    let _ = ascii.push(ch);
                }
                info!("mDNS: {} bytes: {}", n, ascii);

                let current_time = Instant::now().as_millis() as u64;
                let (ip_v4, port, _instance) = self.parse_with_state(
                    &mut q,
                    &rx[..n],
                    current_time,
                    service_name,
                    &mut cached_hostname,
                    &mut cached_port,
                    &mut cached_ip,
                    &mut cache_time,
                );
                info!("mDNS: Got response: {:?} {:?} {:?}", ip_v4, port, _instance);

                if port != 0 && ip_v4 != [0, 0, 0, 0] {
                    info!("mDNS: Got result: {:?} {:?}", ip_v4, port);
                    return (
                        IpAddr::V4(Ipv4Addr::new(ip_v4[0], ip_v4[1], ip_v4[2], ip_v4[3])),
                        port,
                    );
                }
            }
            if Instant::now() >= deadline {
                // no result yet: back off a bit, then extend the window
                Timer::after(Duration::from_millis(250)).await;
            }
            Timer::after(Duration::from_millis(40)).await;
        }
    }

    /// Private helper method to handle stateful parsing logic
    fn parse_with_state(
        &self,
        q: &mut MdnsQuery,
        data: &[u8],
        current_time: u64,
        service_name: &'static str,
        cached_hostname: &mut Option<heapless::String<64>>,
        cached_port: &mut Option<u16>,
        cached_ip: &mut Option<[u8; 4]>,
        cache_time: &mut Option<u64>,
    ) -> ([u8; 4], u16, Option<&str>) {
        // Check for cache timeout and cleanup (30 seconds = 30000 milliseconds)
        if let Some(cache_timestamp) = *cache_time {
            if current_time.saturating_sub(cache_timestamp) > 30000 {
                info!("mDNS: Cache cleanup - timeout reached after {}ms, clearing cached data (hostname: {:?}, port: {:?}, ip: {:?})", 
                      current_time.saturating_sub(cache_timestamp), cached_hostname, cached_port, cached_ip);
                *cached_hostname = None;
                *cached_port = None;
                *cached_ip = None;
                *cache_time = None;
            }
        }

        // First try the existing bundled parsing approach (for Bonjour compatibility)
        let (ip_v4, port, _instance) = q.parse_mdns_query(data, None);

        // If we got a complete result, return it immediately
        if port != 0 && ip_v4 != [0, 0, 0, 0] {
            info!(
                "mDNS: Complete service resolution via bundled records - IP: {:?}, port: {}",
                ip_v4, port
            );
            return (ip_v4, port, None);
        }

        // If no complete result, try to parse individual SRV records for caching
        if let Some((srv_hostname, srv_port)) = self.parse_srv_record(data) {
            info!("mDNS: SRV record cached - service: {}, target hostname: {}, port: {}, timestamp: {}", 
                  service_name, srv_hostname, srv_port, current_time);

            // Check if we already have a matching A record cached
            if let Some(cached_ip_addr) = cached_ip {
                info!("mDNS: Complete service resolution via cached A record match - hostname: {}, IP: {:?}, port: {}", 
                      srv_hostname, cached_ip_addr, srv_port);
                return (*cached_ip_addr, srv_port, None);
            }

            // Cache the SRV record data
            *cached_hostname = Some(srv_hostname);
            *cached_port = Some(srv_port);
            *cache_time = Some(current_time);
        }

        // Try to parse A records and match with cached SRV data
        if let Some((a_hostname, a_ip)) = self.parse_a_record(data) {
            info!(
                "mDNS: A record received - hostname: {}, IP address: {}.{}.{}.{}",
                a_hostname, a_ip[0], a_ip[1], a_ip[2], a_ip[3]
            );

            // Check if this A record matches a cached SRV record
            if let (Some(cached_srv_hostname), Some(cached_srv_port)) =
                (cached_hostname.as_ref(), cached_port)
            {
                if a_hostname == *cached_srv_hostname {
                    info!("mDNS: Complete service resolution via A record match - hostname: {}, IP: {}.{}.{}.{}, port: {}", 
                          a_hostname, a_ip[0], a_ip[1], a_ip[2], a_ip[3], cached_srv_port);
                    return (a_ip, *cached_srv_port, None);
                } else {
                    info!(
                        "mDNS: A record hostname '{}' does not match cached SRV hostname '{}'",
                        a_hostname, cached_srv_hostname
                    );
                }
            }

            // Cache the A record data for potential future SRV matching
            *cached_ip = Some(a_ip);
            if cache_time.is_none() {
                *cache_time = Some(current_time);
                info!("mDNS: A record cached for future SRV matching - hostname: {}, IP: {}.{}.{}.{}, timestamp: {}", 
                      a_hostname, a_ip[0], a_ip[1], a_ip[2], a_ip[3], current_time);
            }
        }

        // Return the original result (likely empty if we got here)
        (ip_v4, port, None)
    }

    /// Parse SRV record from DNS packet data
    /// Returns (hostname, port) if SRV record is found, None otherwise
    fn parse_srv_record(&self, data: &[u8]) -> Option<(heapless::String<64>, u16)> {
        if data.len() < 12 {
            return None; // Too short to be a valid DNS packet
        }

        // Skip DNS header (12 bytes)
        let mut offset = 12;

        // Skip question section - we need to find the answer section
        let question_count = u16::from_be_bytes([data[4], data[5]]);
        for _ in 0..question_count {
            // Skip question name
            offset = self.skip_dns_name(data, offset)?;
            // Skip QTYPE (2 bytes) and QCLASS (2 bytes)
            offset += 4;
        }

        // Parse answer section
        let answer_count = u16::from_be_bytes([data[6], data[7]]);
        for _ in 0..answer_count {
            let _record_start = offset;

            // Skip name
            offset = self.skip_dns_name(data, offset)?;

            if offset + 10 > data.len() {
                return None;
            }

            // Read TYPE (2 bytes)
            let record_type = u16::from_be_bytes([data[offset], data[offset + 1]]);
            offset += 2;

            // Skip CLASS (2 bytes) and TTL (4 bytes)
            offset += 6;

            // Read RDLENGTH (2 bytes)
            let rdlength = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
            offset += 2;

            if record_type == 33 {
                // SRV record type
                return self.parse_srv_rdata(data, offset, rdlength);
            }

            // Skip to next record
            offset += rdlength;
        }

        None
    }

    /// Parse SRV record RDATA section
    fn parse_srv_rdata(
        &self,
        data: &[u8],
        offset: usize,
        rdlength: usize,
    ) -> Option<(heapless::String<64>, u16)> {
        if rdlength < 6 || offset + rdlength > data.len() {
            return None;
        }

        // SRV RDATA format: Priority (2) + Weight (2) + Port (2) + Target (variable)
        // Skip Priority (2 bytes) and Weight (2 bytes)
        let port_offset = offset + 4;
        let port = u16::from_be_bytes([data[port_offset], data[port_offset + 1]]);

        // Parse target hostname starting at offset + 6
        let hostname_offset = offset + 6;
        if let Some(hostname) = self.parse_dns_name(data, hostname_offset) {
            Some((hostname, port))
        } else {
            None
        }
    }

    /// Skip over a DNS name in the packet, handling compression
    fn skip_dns_name(&self, data: &[u8], mut offset: usize) -> Option<usize> {
        loop {
            if offset >= data.len() {
                return None;
            }

            let len = data[offset];

            if len == 0 {
                // End of name
                return Some(offset + 1);
            } else if len & 0xC0 == 0xC0 {
                // Compression pointer - skip 2 bytes and we're done
                return Some(offset + 2);
            } else {
                // Regular label - skip length byte + label bytes
                offset += 1 + len as usize;
            }
        }
    }

    /// Parse A record from DNS packet data
    /// Returns (hostname, ip_address) if A record is found, None otherwise
    fn parse_a_record(&self, data: &[u8]) -> Option<(heapless::String<64>, [u8; 4])> {
        if data.len() < 12 {
            return None; // Too short to be a valid DNS packet
        }

        // Skip DNS header (12 bytes)
        let mut offset = 12;

        // Skip question section - we need to find the answer section
        let question_count = u16::from_be_bytes([data[4], data[5]]);
        for _ in 0..question_count {
            // Skip question name
            offset = self.skip_dns_name(data, offset)?;
            // Skip QTYPE (2 bytes) and QCLASS (2 bytes)
            offset += 4;
        }

        // Parse answer section
        let answer_count = u16::from_be_bytes([data[6], data[7]]);
        for _ in 0..answer_count {
            let _record_start = offset;

            // Parse and store the name for this record
            let hostname = self.parse_dns_name(data, offset)?;

            // Skip name
            offset = self.skip_dns_name(data, offset)?;

            if offset + 10 > data.len() {
                return None;
            }

            // Read TYPE (2 bytes)
            let record_type = u16::from_be_bytes([data[offset], data[offset + 1]]);
            offset += 2;

            // Skip CLASS (2 bytes) and TTL (4 bytes)
            offset += 6;

            // Read RDLENGTH (2 bytes)
            let rdlength = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
            offset += 2;

            if record_type == 1 && rdlength == 4 {
                // A record type with 4-byte IPv4 address
                if offset + 4 <= data.len() {
                    let ip_addr = [
                        data[offset],
                        data[offset + 1],
                        data[offset + 2],
                        data[offset + 3],
                    ];
                    return Some((hostname, ip_addr));
                }
            }

            // Skip to next record
            offset += rdlength;
        }

        None
    }

    /// Parse a DNS name from the packet, handling compression
    fn parse_dns_name(&self, data: &[u8], mut offset: usize) -> Option<heapless::String<64>> {
        let mut result = heapless::String::<64>::new();
        let mut first_label = true;

        loop {
            if offset >= data.len() {
                return None;
            }

            let len = data[offset];

            if len == 0 {
                // End of name
                break;
            } else if len & 0xC0 == 0xC0 {
                // Compression pointer - follow it
                if offset + 1 >= data.len() {
                    return None;
                }
                let pointer = ((len as u16 & 0x3F) << 8) | data[offset + 1] as u16;
                offset = pointer as usize;
                continue;
            } else {
                // Regular label
                if !first_label {
                    if result.push('.').is_err() {
                        return None; // String full
                    }
                }
                first_label = false;

                offset += 1;
                if offset + len as usize > data.len() {
                    return None;
                }

                for i in 0..len {
                    let ch = data[offset + i as usize] as char;
                    if result.push(ch).is_err() {
                        return None; // String full
                    }
                }
                offset += len as usize;
            }
        }

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }
}
