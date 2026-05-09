use std::net::Ipv4Addr;
use std::thread::sleep;
use std::time::Duration;
use anyhow::Context;
use clap::{Args, Parser, Subcommand};
use fake::Fake;
use fake::locales::EN;
use log::{error, info, warn};
use pnet::datalink::{channel, interfaces, Channel, Config, DataLinkSender, MacAddr};
use pnet::packet::ethernet::{EtherTypes, MutableEthernetPacket};
use pnet::packet::ip::IpNextHeaderProtocols;
use pnet::packet::ipv4::MutableIpv4Packet;
use pnet::packet::MutablePacket;
use pnet::packet::tcp::{MutableTcpPacket, TcpFlags};
use simple_logger::SimpleLogger;

fn main() -> Result<(), anyhow::Error> {
    SimpleLogger::new().env().without_timestamps().init()?;

    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Run(opt)) => {
            return run(opt)
        },
        None => { }
    }

    Ok(())
}

#[derive(Parser)]
#[command(version, about, long_about = None, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>
}

#[derive(Subcommand)]
enum Commands {
    Run(RunOpt)
}

#[derive(Args)]
struct RunOpt {
    #[arg(short='i', long)]
    interface: String
}

#[derive(Clone, Copy)]
struct Endpoint {
    mac: MacAddr,
    ip: Ipv4Addr,
    port: u16,
}

fn run(opt: RunOpt) -> Result<(), anyhow::Error> {
    info!("Running on {}", opt.interface);

    let interface = interfaces()
        .into_iter()
        .find(|iface| iface.name == opt.interface)
        .context("No such interface")?;

    let config = Config::default();

    let (mut tx, rx) = match channel(&interface, config) {
        Ok(Channel::Ethernet(tx, rx)) => (tx, rx),
        Ok(_) => {
            warn!("Unhandled channel type");
            return Ok(())
        }
        Err(e) => {
            error!("Failed to create datelink channel: {e}");
            return Ok(())
        }
    };

        let client_macaddr = fake::faker::internet::raw::MACAddress(EN).fake::<String>();
        let client_ip = fake::faker::internet::raw::IPv4(EN).fake::<String>();
        let client = Endpoint {
            mac: client_macaddr.parse().context("invalid generated MAC address")?,
            ip: client_ip.parse().context("invalid generated IPv4 address")?,
            port: (10000..50000).fake::<u16>()
        };

        let server_macaddr = fake::faker::internet::raw::MACAddress(EN).fake::<String>();
        let server_ip = fake::faker::internet::raw::IPv4(EN).fake::<String>();
        let server = Endpoint {
            mac: server_macaddr.parse().context("invalid generated server MAC address")?,
            ip: server_ip.parse().context("invalid generated server IPv4 address")?,
            port: 80
        };

    loop {
        let cseq = 1000;
        let sseq = 5000;

        // 1. Client -> Server: SYN
        send_packet(&mut tx, client, server, cseq, 0, TcpFlags::SYN, b"");

        // 2. Server -> Client: SYN/ACK
        send_packet(&mut tx, server, client, sseq, cseq + 1, TcpFlags::SYN | TcpFlags::ACK, b"");

        // 3. Client -> Server: ACK
        send_packet(&mut tx, client, server, cseq + 1, sseq + 1, TcpFlags::ACK, b"");

        let http_body = b"Hello world!";
        let http_req = format!(
            "POST /test HTTP/1.1\r\n\
Host: example.local\r\n\
Connection: keep-alive\r\n\
Keep-Alive: timeout=5, max=10\r\n\
User-Agent: zeek-rust-test\r\n\
Accept: */*\r\n\
Content-Length: {}\r\n\
\r\n",
            http_body.len()
        );
        let mut http_req = http_req.into_bytes();
        http_req.extend_from_slice(http_body);

        // 4. Client -> Server: HTTP Request
        send_packet(&mut tx, client, server, cseq + 1, sseq + 1, TcpFlags::PSH | TcpFlags::ACK, &http_req);

        let http_resp = b"HTTP/1.1 200 OK\r\n\
Content-Type: text/plain\r\n\
Content-Length: 5\r\n\
\r\n\
hello";

        // 5. Server -> Client: HTTP Response
        send_packet(&mut tx, server, client, sseq + 1, cseq + 1 + http_req.len() as u32, TcpFlags::PSH | TcpFlags::ACK, http_resp);

        // 6) FIN/ACK (server -> client)
        send_packet(&mut tx, server, client, sseq + 1 + http_resp.len() as u32, cseq + 1 + http_req.len() as u32, TcpFlags::FIN | TcpFlags::ACK, b"", );

        // 7) ACK (client -> server)
        send_packet(&mut tx, client, server, cseq + 1 + http_req.len() as u32, sseq + 2 + http_resp.len() as u32, TcpFlags::ACK, b"", );

        // 8) FIN/ACK (client -> server)
        send_packet(&mut tx, client, server, cseq + 1 + http_req.len() as u32, sseq + 2 + http_resp.len() as u32, TcpFlags::FIN | TcpFlags::ACK, b"", );

        // 9) ACK (server -> client)
        send_packet(&mut tx, server, client, sseq + 2 + http_resp.len() as u32, cseq + 2 + http_req.len() as u32, TcpFlags::ACK, b"", );

        info!("Sent http request & response");

        sleep(Duration::from_secs(1));
    }

    Ok(())
}

const ETH_LEN: usize = 14;
const IPV4_LEN: usize = 20;
const TCP_LEN: usize = 20;

fn build_tcp_frame(
    buf: &mut [u8],
    src: Endpoint,
    dst: Endpoint,
    seq: u32,
    ack: u32,
    flags: u8,
    payload: &[u8]
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
        ip.set_identification(1234);
        ip.set_flags(2);

        let checksum = pnet::packet::ipv4::checksum(&ip.to_immutable());
        ip.set_checksum(checksum);
    }

    {
        let tcp_start = ETH_LEN + IPV4_LEN;
        let payload_start = tcp_start + TCP_LEN;

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
    tx: &mut Box<dyn DataLinkSender>,
    src: Endpoint,
    dst: Endpoint,
    seq: u32,
    ack: u32,
    flags: u8,
    payload: &[u8]
) {
    let mut buf = vec![0u8; ETH_LEN + IPV4_LEN + TCP_LEN + payload.len()];
    let size = build_tcp_frame(&mut buf, src, dst, seq, ack, flags, payload);
    tx.send_to(&buf[..size], None).unwrap().unwrap();
    
    sleep(Duration::from_millis(10));
}
