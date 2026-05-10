use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, anyhow};
use pnet::datalink::{Channel, DataLinkSender, MacAddr, channel, interfaces};
use pnet::packet::MutablePacket;
use pnet::packet::ethernet::{EtherTypes, MutableEthernetPacket};
use pnet::packet::ip::IpNextHeaderProtocols;
use pnet::packet::ipv4::MutableIpv4Packet;
use pnet::packet::tcp::{MutableTcpPacket, TcpFlags};
use rand::RngExt;

use crate::config::AppConfigEndpoint;
use crate::vuser::{SampledHttpReq, SampledHttpRes};

const ETH_LEN: usize = 14;
const IPV4_LEN: usize = 20;
const TCP_LEN: usize = 20;

#[derive(Clone)]
pub struct Transport {
    inner: Arc<TransportInner>,
}

struct TransportInner {
    tx: Mutex<Box<dyn DataLinkSender>>,
    src: EndpointSpec,
    dst: EndpointSpec,
    stats: TransportStatsInner,
    ip_id: AtomicU16,
}

struct TransportStatsInner {
    packets: AtomicU64,
    bytes: AtomicU64,
    sessions: AtomicU64,
}

#[derive(Clone)]
struct EndpointSpec {
    cidr: cidr::Ipv4Cidr,
    port: PortRange,
}

#[derive(Clone, Copy)]
struct PortRange {
    start: u16,
    end: u16,
}

#[derive(Clone, Copy)]
pub struct Endpoint {
    pub mac: MacAddr,
    pub ip: Ipv4Addr,
    pub port: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct TcpSequence {
    pub client: u32,
    pub server: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TransportStats {
    pub packets: u64,
    pub bytes: u64,
    pub sessions: u64,
}

impl Transport {
    pub fn open(
        interface_name: &str,
        src: AppConfigEndpoint,
        dst: AppConfigEndpoint,
    ) -> Result<Self, anyhow::Error> {
        let interface = interfaces()
            .into_iter()
            .find(|iface| iface.name == interface_name)
            .with_context(|| format!("no such interface: {interface_name}"))?;

        let (tx, _) = match channel(&interface, pnet::datalink::Config::default()) {
            Ok(Channel::Ethernet(tx, rx)) => (tx, rx),
            Ok(_) => return Err(anyhow!("unsupported datalink channel type")),
            Err(err) => return Err(anyhow!("failed to create datalink channel: {err}")),
        };

        Ok(Self {
            inner: Arc::new(TransportInner {
                tx: Mutex::new(tx),
                src: EndpointSpec::try_from(src)?,
                dst: EndpointSpec::try_from(dst)?,
                stats: TransportStatsInner::default(),
                ip_id: AtomicU16::new(rand::rng().random()),
            }),
        })
    }

    pub fn allocate_client(&self) -> Endpoint {
        self.inner.src.sample()
    }

    pub fn send_session_with_client(
        &self,
        client: Endpoint,
        seq: TcpSequence,
        req: &SampledHttpReq,
        res: &SampledHttpRes,
    ) -> Result<(), anyhow::Error> {
        let server = self.inner.dst.sample();
        let req_bytes = build_http_request(req);
        let res_bytes = build_http_response(res);

        let cseq = seq.client;
        let sseq = seq.server;
        let mut tx = self
            .inner
            .tx
            .lock()
            .map_err(|_| anyhow!("datalink sender mutex poisoned"))?;

        send_packet(
            &mut **tx,
            client,
            server,
            cseq,
            0,
            TcpFlags::SYN,
            self.next_ip_id(),
            b"",
        )?;
        self.record_packet(ETH_LEN + IPV4_LEN + TCP_LEN);
        send_packet(
            &mut **tx,
            server,
            client,
            sseq,
            cseq + 1,
            TcpFlags::SYN | TcpFlags::ACK,
            self.next_ip_id(),
            b"",
        )?;
        self.record_packet(ETH_LEN + IPV4_LEN + TCP_LEN);
        send_packet(
            &mut **tx,
            client,
            server,
            cseq + 1,
            sseq + 1,
            TcpFlags::ACK,
            self.next_ip_id(),
            b"",
        )?;
        self.record_packet(ETH_LEN + IPV4_LEN + TCP_LEN);
        send_packet(
            &mut **tx,
            client,
            server,
            cseq + 1,
            sseq + 1,
            TcpFlags::PSH | TcpFlags::ACK,
            self.next_ip_id(),
            &req_bytes,
        )?;
        self.record_packet(ETH_LEN + IPV4_LEN + TCP_LEN + req_bytes.len());
        send_packet(
            &mut **tx,
            server,
            client,
            sseq + 1,
            cseq + 1 + req_bytes.len() as u32,
            TcpFlags::PSH | TcpFlags::ACK,
            self.next_ip_id(),
            &res_bytes,
        )?;
        self.record_packet(ETH_LEN + IPV4_LEN + TCP_LEN + res_bytes.len());
        send_packet(
            &mut **tx,
            server,
            client,
            sseq + 1 + res_bytes.len() as u32,
            cseq + 1 + req_bytes.len() as u32,
            TcpFlags::FIN | TcpFlags::ACK,
            self.next_ip_id(),
            b"",
        )?;
        self.record_packet(ETH_LEN + IPV4_LEN + TCP_LEN);
        send_packet(
            &mut **tx,
            client,
            server,
            cseq + 1 + req_bytes.len() as u32,
            sseq + 2 + res_bytes.len() as u32,
            TcpFlags::ACK,
            self.next_ip_id(),
            b"",
        )?;
        self.record_packet(ETH_LEN + IPV4_LEN + TCP_LEN);
        send_packet(
            &mut **tx,
            client,
            server,
            cseq + 1 + req_bytes.len() as u32,
            sseq + 2 + res_bytes.len() as u32,
            TcpFlags::FIN | TcpFlags::ACK,
            self.next_ip_id(),
            b"",
        )?;
        self.record_packet(ETH_LEN + IPV4_LEN + TCP_LEN);
        send_packet(
            &mut **tx,
            server,
            client,
            sseq + 2 + res_bytes.len() as u32,
            cseq + 2 + req_bytes.len() as u32,
            TcpFlags::ACK,
            self.next_ip_id(),
            b"",
        )?;
        self.record_packet(ETH_LEN + IPV4_LEN + TCP_LEN);
        self.inner.stats.sessions.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    pub fn stats(&self) -> TransportStats {
        TransportStats {
            packets: self.inner.stats.packets.load(Ordering::Relaxed),
            bytes: self.inner.stats.bytes.load(Ordering::Relaxed),
            sessions: self.inner.stats.sessions.load(Ordering::Relaxed),
        }
    }

    fn record_packet(&self, bytes: usize) {
        self.inner.stats.packets.fetch_add(1, Ordering::Relaxed);
        self.inner
            .stats
            .bytes
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }

    fn next_ip_id(&self) -> u16 {
        self.inner.ip_id.fetch_add(1, Ordering::Relaxed)
    }
}

impl Default for TransportStatsInner {
    fn default() -> Self {
        Self {
            packets: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            sessions: AtomicU64::new(0),
        }
    }
}

impl TryFrom<AppConfigEndpoint> for EndpointSpec {
    type Error = anyhow::Error;

    fn try_from(value: AppConfigEndpoint) -> Result<Self, Self::Error> {
        Ok(Self {
            cidr: value.cidr,
            port: PortRange::parse(&value.port)?,
        })
    }
}

impl EndpointSpec {
    fn sample(&self) -> Endpoint {
        Endpoint {
            mac: random_mac(),
            ip: sample_ip(self.cidr),
            port: self.port.sample(),
        }
    }
}

impl PortRange {
    fn parse(input: &str) -> Result<Self, anyhow::Error> {
        if let Some((start, end)) = input.split_once('-') {
            let start = start.trim().parse::<u16>()?;
            let end = end.trim().parse::<u16>()?;

            anyhow::ensure!(start <= end, "invalid port range: {input}");

            return Ok(Self { start, end });
        }

        let port = input.trim().parse::<u16>()?;
        Ok(Self {
            start: port,
            end: port,
        })
    }

    fn sample(self) -> u16 {
        if self.start == self.end {
            return self.start;
        }

        rand::rng().random_range(self.start..=self.end)
    }
}

fn build_http_request(req: &SampledHttpReq) -> Vec<u8> {
    let mut out = format!(
        "{} {} HTTP/1.1\r\nHost: {}\r\n",
        req.method, req.path, req.host
    );

    for (key, value) in &req.headers {
        out.push_str(key);
        out.push_str(": ");
        out.push_str(value);
        out.push_str("\r\n");
    }

    if !has_header(&req.headers, "Content-Length") {
        out.push_str(&format!("Content-Length: {}\r\n", req.body.len()));
    }

    out.push_str("\r\n");
    let mut bytes = out.into_bytes();
    bytes.extend_from_slice(req.body.as_bytes());
    bytes
}

fn build_http_response(res: &SampledHttpRes) -> Vec<u8> {
    let mut out = format!("{}\r\n", res.status);

    for (key, value) in &res.headers {
        out.push_str(key);
        out.push_str(": ");
        out.push_str(value);
        out.push_str("\r\n");
    }

    if !has_header(&res.headers, "Content-Length") {
        out.push_str(&format!("Content-Length: {}\r\n", res.body.len()));
    }

    out.push_str("\r\n");
    let mut bytes = out.into_bytes();
    bytes.extend_from_slice(res.body.as_bytes());
    bytes
}

fn has_header(headers: &[(String, String)], name: &str) -> bool {
    headers
        .iter()
        .any(|(key, _)| key.eq_ignore_ascii_case(name))
}

fn sample_ip(cidr: cidr::Ipv4Cidr) -> Ipv4Addr {
    let first = u32::from(cidr.first_address());
    let last = u32::from(cidr.last_address());

    if first == last {
        return Ipv4Addr::from(first);
    }

    Ipv4Addr::from(rand::rng().random_range(first..=last))
}

fn random_mac() -> MacAddr {
    let mut rng = rand::rng();
    MacAddr::new(
        0x02,
        rng.random(),
        rng.random(),
        rng.random(),
        rng.random(),
        rng.random(),
    )
}

fn build_tcp_frame(
    buf: &mut [u8],
    src: Endpoint,
    dst: Endpoint,
    seq: u32,
    ack: u32,
    flags: u8,
    ip_id: u16,
    payload: &[u8],
) -> usize {
    let total_len = ETH_LEN + IPV4_LEN + TCP_LEN + payload.len();

    {
        let mut eth = MutableEthernetPacket::new(&mut buf[..total_len]).unwrap();
        eth.set_source(src.mac);
        eth.set_destination(dst.mac);
        eth.set_ethertype(EtherTypes::Ipv4);
    }

    {
        let ip_start = ETH_LEN;
        let tcp_start = ETH_LEN + IPV4_LEN;

        let mut ip = MutableIpv4Packet::new(&mut buf[ip_start..tcp_start]).unwrap();
        ip.set_version(4);
        ip.set_header_length(5);
        ip.set_total_length((IPV4_LEN + TCP_LEN + payload.len()) as u16);
        ip.set_ttl(64);
        ip.set_next_level_protocol(IpNextHeaderProtocols::Tcp);
        ip.set_source(src.ip);
        ip.set_destination(dst.ip);
        ip.set_identification(ip_id);
        ip.set_flags(2);

        let checksum = pnet::packet::ipv4::checksum(&ip.to_immutable());
        ip.set_checksum(checksum);
    }

    {
        let tcp_start = ETH_LEN + IPV4_LEN;

        let mut tcp = MutableTcpPacket::new(&mut buf[tcp_start..total_len]).unwrap();
        tcp.set_source(src.port);
        tcp.set_destination(dst.port);
        tcp.set_sequence(seq);
        tcp.set_acknowledgement(ack);
        tcp.set_data_offset(5);
        tcp.set_flags(flags);
        tcp.set_window(64240);
        tcp.set_urgent_ptr(0);

        tcp.payload_mut().copy_from_slice(payload);

        let checksum = pnet::packet::tcp::ipv4_checksum(&tcp.to_immutable(), &src.ip, &dst.ip);
        tcp.set_checksum(checksum);
    }

    total_len
}

fn send_packet(
    tx: &mut dyn DataLinkSender,
    src: Endpoint,
    dst: Endpoint,
    seq: u32,
    ack: u32,
    flags: u8,
    ip_id: u16,
    payload: &[u8],
) -> Result<(), anyhow::Error> {
    let mut buf = vec![0u8; ETH_LEN + IPV4_LEN + TCP_LEN + payload.len()];
    let size = build_tcp_frame(&mut buf, src, dst, seq, ack, flags, ip_id, payload);

    tx.send_to(&buf[..size], None)
        .ok_or_else(|| anyhow!("datalink sender does not support send_to"))?
        .context("failed to send packet")?;

    Ok(())
}
