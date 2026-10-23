use std::collections::BTreeSet;
use std::io::{self, ErrorKind};
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

const REPLY_WINDOW: Duration = Duration::from_millis(800);

#[derive(Default)]
struct EchoStats {
    targets: usize,
    sent: usize,
    send_errors: usize,
    received: usize,
    rejected: usize,
    receive_errors: usize,
}

impl EchoStats {
    fn summary(&self, matches: usize) -> String {
        format!(
            "ICMP 目标 {}，发出 {}，回包 {}，有效设备 {}，非匹配 {}，发送失败 {}，接收失败 {}",
            self.targets,
            self.sent,
            self.received,
            matches,
            self.rejected,
            self.send_errors,
            self.receive_errors
        )
    }
}

pub(super) struct EchoResult {
    pub addresses: Vec<Ipv4Addr>,
    pub summary: String,
}

fn checksum(bytes: &[u8]) -> u16 {
    let mut sum: u32 = bytes
        .chunks(2)
        .map(|pair| u16::from_be_bytes([pair[0], *pair.get(1).unwrap_or(&0)]) as u32)
        .sum();
    while sum > 0xffff {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

fn request(target: Ipv4Addr, nonce: &[u8; 16]) -> Vec<u8> {
    let address = target.octets();
    let mut packet = vec![8, 0, 0, 0, 0x48, 0x4e, address[2], address[3]];
    packet.extend_from_slice(nonce);
    packet.extend_from_slice(&address);
    let value = checksum(&packet).to_be_bytes();
    packet[2..4].copy_from_slice(&value);
    packet
}

fn valid_reply(
    packet: &[u8],
    sender: Ipv4Addr,
    nonce: &[u8; 16],
    sent: &BTreeSet<Ipv4Addr>,
) -> bool {
    if !sent.contains(&sender) {
        return false;
    }
    // Darwin datagram ICMP sockets include an IPv4 header; other stacks may not.
    let offset = if packet.first().is_some_and(|byte| byte >> 4 == 4) {
        let length = (packet[0] & 0x0f) as usize * 4;
        if length < 20 || packet.len() < length || packet.get(9) != Some(&1) {
            return false;
        }
        length
    } else {
        0
    };
    let Some(echo) = packet.get(offset..) else {
        return false;
    };
    echo.len() == 28
        && echo[0] == 0
        && echo[1] == 0
        && echo[6..8] == sender.octets()[2..4]
        && echo[8..24] == nonce[..]
        && echo[24..28] == sender.octets()
}

fn drain(
    socket: &UdpSocket,
    nonce: &[u8; 16],
    sent: &BTreeSet<Ipv4Addr>,
    replies: &mut BTreeSet<Ipv4Addr>,
    stats: &mut EchoStats,
) -> io::Result<()> {
    let mut buffer = [0_u8; 2048];
    for _ in 0..128 {
        match socket.recv_from(&mut buffer) {
            Ok((length, std::net::SocketAddr::V4(sender))) => {
                stats.received += 1;
                if valid_reply(&buffer[..length], *sender.ip(), nonce, sent) {
                    replies.insert(*sender.ip());
                } else {
                    stats.rejected += 1;
                }
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::WouldBlock => break,
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::ConnectionRefused | ErrorKind::HostUnreachable
                ) =>
            {
                stats.receive_errors += 1;
                continue;
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => {
                stats.receive_errors += 1;
                return Err(io::Error::new(
                    error.kind(),
                    format!("{error}；{}", stats.summary(replies.len())),
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn discover(socket: UdpSocket, targets: &[Ipv4Addr]) -> io::Result<EchoResult> {
    socket.set_nonblocking(true)?;
    let nonce = *uuid::Uuid::new_v4().as_bytes();
    let mut sent = BTreeSet::new();
    let mut replies = BTreeSet::new();
    let mut last_error = None;
    let mut stats = EchoStats {
        targets: targets.len(),
        ..EchoStats::default()
    };
    for target in targets {
        match socket.send_to(&request(*target, &nonce), SocketAddrV4::new(*target, 0)) {
            Ok(_) => {
                sent.insert(*target);
                stats.sent += 1;
            }
            Err(error) => {
                stats.send_errors += 1;
                last_error = Some(error);
            }
        }
        drain(&socket, &nonce, &sent, &mut replies, &mut stats)?;
        std::thread::sleep(Duration::from_millis(1));
    }
    if sent.is_empty() {
        return last_error.map_or_else(
            || {
                Ok(EchoResult {
                    addresses: Vec::new(),
                    summary: stats.summary(0),
                })
            },
            |error| {
                Err(io::Error::new(
                    error.kind(),
                    format!("{error}；{}", stats.summary(0)),
                ))
            },
        );
    }
    let deadline = Instant::now() + REPLY_WINDOW;
    while Instant::now() < deadline {
        drain(&socket, &nonce, &sent, &mut replies, &mut stats)?;
        if replies.len() == sent.len() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut summary = stats.summary(replies.len());
    if let Some(error) = last_error {
        summary.push_str(&format!("；最后发送错误：{error}"));
    }
    Ok(EchoResult {
        addresses: replies.into_iter().collect(),
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn probe_summary_distinguishes_sending_from_verified_replies() {
        let stats = EchoStats {
            targets: 253,
            sent: 252,
            send_errors: 1,
            received: 5,
            rejected: 4,
            receive_errors: 0,
        };
        let summary = stats.summary(1);
        assert!(summary.contains("发出 252"));
        assert!(summary.contains("有效设备 1"));
        assert!(summary.contains("非匹配 4"));
        assert!(summary.contains("发送失败 1"));
    }
    #[test]
    fn only_correlated_echo_replies_count_as_live_devices() {
        let ip = Ipv4Addr::new(192, 0, 2, 8);
        let nonce = [0x42; 16];
        let sent = BTreeSet::from([ip]);
        let mut packet = request(ip, &nonce);
        assert_eq!(checksum(&packet), 0);
        assert!(!valid_reply(&packet, ip, &nonce, &sent));
        packet[0] = 0;
        assert!(valid_reply(&packet, ip, &nonce, &sent));
        assert!(!valid_reply(&packet, ip, &[0x43; 16], &sent));
        assert!(!valid_reply(
            &packet,
            Ipv4Addr::new(192, 0, 2, 9),
            &nonce,
            &sent
        ));
        packet[0] = 3;
        assert!(!valid_reply(&packet, ip, &nonce, &sent));
    }
    #[test]
    fn darwin_ipv4_headers_and_truncated_packets_are_handled_safely() {
        let ip = Ipv4Addr::new(192, 0, 2, 8);
        let nonce = [7; 16];
        let sent = BTreeSet::from([ip]);
        let mut packet = vec![0; 20];
        packet[0] = 0x45;
        packet[9] = 1;
        let mut reply = request(ip, &nonce);
        reply[0] = 0;
        packet.extend(reply);
        assert!(valid_reply(&packet, ip, &nonce, &sent));
        for end in 0..packet.len() {
            assert!(!valid_reply(&packet[..end], ip, &nonce, &sent));
        }
        packet[9] = 17;
        assert!(!valid_reply(&packet, ip, &nonce, &sent));
    }
}
