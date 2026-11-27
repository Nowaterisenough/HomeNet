use super::NeighborRow;
use std::collections::{HashMap, HashSet};
use std::ffi::CStr;
use std::io;
use std::mem::size_of;
use std::net::{Ipv4Addr, Ipv6Addr, UdpSocket};
use std::os::fd::AsRawFd;

const MAX_TABLE_BYTES: usize = 8 * 1024 * 1024;

// Darwin net/route_private.h. arp -l reads this extended routing message.
const NET_RT_DUMPX_FLAGS: i32 = 9;
const RTM_GET_EXT: u8 = 0x15;

#[repr(C)]
struct ArpRouteHeader {
    length: u16,
    version: u8,
    kind: u8,
    index: u32,
    flags: i32,
    reserved: u32,
    addrs: i32,
    _pid_seq_errno_use_inits: [i32; 5],
    _metrics: libc::rt_metrics,
    refs: u32,
    _probes: u32,
    _send_expire: u64,
    receive_expire: u64,
    _link_metrics: [i32; 3],
}

// Darwin netinet6/nd6.h (in6_nbrinfo) and in6_var.h (_IOWR('i', 78, ...)).
#[repr(C)]
struct NdpInfo {
    interface: [u8; libc::IF_NAMESIZE],
    address: libc::in6_addr,
    asked: libc::c_long,
    is_router: libc::c_int,
    state: libc::c_int,
    expire: libc::c_int,
}
const SIOCGNBRINFO_IN6: libc::c_ulong = 0xc0000000
    | ((size_of::<NdpInfo>() as libc::c_ulong) << 16)
    | ((b'i' as libc::c_ulong) << 8)
    | 78;

fn ndp_state(socket: &UdpSocket, interface: &str, ip: Ipv6Addr, now: i64) -> Option<&'static str> {
    if interface.is_empty() || interface.len() >= libc::IF_NAMESIZE {
        return None;
    }
    let mut info = NdpInfo {
        interface: [0; libc::IF_NAMESIZE],
        address: libc::in6_addr {
            s6_addr: ip.octets(),
        },
        asked: 0,
        is_router: 0,
        state: 0,
        expire: 0,
    };
    info.interface[..interface.len()].copy_from_slice(interface.as_bytes());
    if unsafe { libc::ioctl(socket.as_raw_fd(), SIOCGNBRINFO_IN6, &mut info) } != 0 {
        return None;
    }
    Some(classify_ndp(info.state, i64::from(info.expire), now))
}

fn classify_ndp(state: i32, expiry: i64, now: i64) -> &'static str {
    match (state, expiry) {
        (0, _) => "Incomplete",
        (_, 0) => "Permanent",
        (1, expiry) if expiry > now => "Reachable",
        (3, _) => "Delay",
        (4, _) => "Probe",
        _ => "Stale",
    }
}

pub(super) fn read() -> io::Result<Vec<NeighborRow>> {
    let mut rows = Vec::new();
    let mut last_error = None;
    for family in [libc::AF_INET, libc::AF_INET6] {
        match read_table(family, libc::NET_RT_FLAGS) {
            Ok(table) => rows.extend(parse_table(&table)),
            Err(error) => {
                crate::config::add_log(
                    "warn",
                    "设备发现",
                    &format!("原生邻居表（地址族 {family}）读取失败：{error}"),
                );
                last_error = Some(error);
            }
        }
    }
    if rows.is_empty() {
        if let Some(error) = last_error {
            return Err(error);
        }
    }
    let now = chrono::Utc::now().timestamp();
    let arp_rows = read_table(libc::AF_INET, NET_RT_DUMPX_FLAGS)
        .map(|bytes| parse_routes(&bytes, |message| active_arp_row(message, now)))
        .unwrap_or_default();
    // Require the same interface, IP and MAC across the two snapshots.
    fn identity(row: &NeighborRow) -> (&str, &str, &str) {
        (&row.interface_alias, &row.ip_address, &row.link_layer_address)
    }
    let active_arp: HashSet<_> = arp_rows.iter().map(identity).collect();
    let socket = UdpSocket::bind((Ipv6Addr::UNSPECIFIED, 0)).ok();
    for row in &mut rows {
        if let Ok(ip) = row.ip_address.parse::<Ipv6Addr>() {
            let state = socket.as_ref().and_then(|socket| {
                ndp_state(socket, &row.interface_alias, ip, now)
            });
            if let Some(state) = state {
                row.state = state.into();
            }
        } else if active_arp.contains(&identity(row)) {
            row.state = "Reachable".into();
        }
    }
    Ok(rows)
}

pub(super) fn local_macs() -> io::Result<HashMap<String, String>> {
    let mut head = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return Err(io::Error::last_os_error());
    }
    struct Interfaces(*mut libc::ifaddrs);
    impl Drop for Interfaces {
        fn drop(&mut self) {
            unsafe { libc::freeifaddrs(self.0) };
        }
    }
    let interfaces = Interfaces(head);
    let mut current = interfaces.0;
    let mut result = HashMap::new();
    while !current.is_null() {
        // getifaddrs owns this linked list and each sockaddr until freeifaddrs.
        let interface = unsafe { &*current };
        if !interface.ifa_addr.is_null() && !interface.ifa_name.is_null() {
            let address = unsafe { &*interface.ifa_addr };
            if address.sa_family as i32 == libc::AF_LINK {
                let bytes = unsafe {
                    std::slice::from_raw_parts(
                        interface.ifa_addr.cast::<u8>(),
                        address.sa_len as usize,
                    )
                };
                if let Some(mac) = parse_link_address(bytes).filter(|mac| super::is_usable_mac(mac))
                {
                    let name = unsafe { CStr::from_ptr(interface.ifa_name) }
                        .to_string_lossy()
                        .into_owned();
                    result.insert(name, mac);
                }
            }
        }
        current = interface.ifa_next;
    }
    Ok(result)
}

fn read_table(family: i32, query: i32) -> io::Result<Vec<u8>> {
    let mut mib = [
        libc::CTL_NET,
        libc::PF_ROUTE,
        0,
        family,
        query,
        libc::RTF_LLINFO,
    ];
    // Read under HomeNet's own identity instead of relying on a child tool's access.
    for _ in 0..3 {
        let mut length = 0;
        let result = unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                mib.len() as u32,
                std::ptr::null_mut(),
                &mut length,
                std::ptr::null_mut(),
                0,
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        if length == 0 {
            return Ok(Vec::new());
        }
        if length > MAX_TABLE_BYTES {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "邻居表过大"));
        }
        let mut bytes = vec![0_u8; length];
        let result = unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                mib.len() as u32,
                bytes.as_mut_ptr().cast(),
                &mut length,
                std::ptr::null_mut(),
                0,
            )
        };
        if result == 0 && length <= bytes.len() {
            bytes.truncate(length);
            return Ok(bytes);
        }
        let error = io::Error::last_os_error();
        if result != 0 && error.raw_os_error() != Some(libc::ENOMEM) {
            return Err(error);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::WouldBlock,
        "邻居表持续变化，请稍后重试",
    ))
}

fn parse_table(bytes: &[u8]) -> Vec<NeighborRow> {
    parse_routes(bytes, |message| {
        if message.len() < size_of::<libc::rt_msghdr>() {
            return None;
        }
        let header = unsafe { message.as_ptr().cast::<libc::rt_msghdr>().read_unaligned() };
        if header.rtm_flags & libc::RTF_LLINFO == 0 {
            return None;
        }
        parse_addresses(
            header.rtm_index.into(),
            header.rtm_addrs,
            &message[size_of::<libc::rt_msghdr>()..],
        )
    })
}

fn active_arp_row(message: &[u8], now: i64) -> Option<NeighborRow> {
    if message.len() < size_of::<ArpRouteHeader>() {
        return None;
    }
    let header = unsafe { message.as_ptr().cast::<ArpRouteHeader>().read_unaligned() };
    if header.kind != RTM_GET_EXT
        || header.flags & libc::RTF_LLINFO == 0
        || header.refs == 0
        || header.receive_expire <= u64::try_from(now).ok()?
    {
        return None;
    }
    parse_addresses(
        header.index,
        header.addrs,
        &message[size_of::<ArpRouteHeader>()..],
    )
}

fn parse_routes(mut bytes: &[u8], parse: impl Fn(&[u8]) -> Option<NeighborRow>) -> Vec<NeighborRow> {
    let mut rows = Vec::new();
    while bytes.len() >= 4 {
        let length = u16::from_ne_bytes([bytes[0], bytes[1]]) as usize;
        if length < 4 || length > bytes.len() {
            break;
        }
        if bytes[2] == libc::RTM_VERSION as u8 {
            if let Some(row) = parse(&bytes[..length]).filter(NeighborRow::is_usable) {
                rows.push(row);
            }
        }
        bytes = &bytes[length..];
    }
    rows
}

fn parse_addresses(ifindex: u32, addrs: i32, bytes: &[u8]) -> Option<NeighborRow> {
    let mut cursor = 0usize;
    let (mut ip, mut mac) = (None, None);
    for index in 0..libc::RTAX_MAX {
        if addrs & (1 << index) == 0 {
            continue;
        }
        let length = *bytes.get(cursor)? as usize;
        let address = bytes.get(cursor..cursor.checked_add(length)?)?;
        if index == libc::RTAX_DST {
            ip = parse_ip(address);
        } else if index == libc::RTAX_GATEWAY {
            mac = parse_link_address(address);
        }
        // Darwin routing messages pad sockaddr records to a 32-bit boundary.
        cursor = cursor.checked_add(length.max(1).div_ceil(4) * 4)?;
        if cursor > bytes.len() {
            return None;
        }
    }
    let mut name = [0_i8; libc::IF_NAMESIZE];
    let interface = unsafe { libc::if_indextoname(ifindex, name.as_mut_ptr()) };
    let interface_alias = if interface.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(name.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    };
    Some(NeighborRow {
        ip_address: ip?,
        link_layer_address: mac?,
        // The route expiry is a cache lifetime, not a reachability confirmation.
        state: "Cached".into(),
        interface_alias,
        _address_family: None,
    })
}

fn parse_ip(address: &[u8]) -> Option<String> {
    match *address.get(1)? as i32 {
        libc::AF_INET => {
            let octets: [u8; 4] = address.get(4..8)?.try_into().ok()?;
            Some(Ipv4Addr::from(octets).to_string())
        }
        libc::AF_INET6 => {
            let mut octets: [u8; 16] = address.get(8..24)?.try_into().ok()?;
            // Link-local route addresses may embed the interface index in bytes 2..4.
            if octets[0] == 0xfe && octets[1] & 0xc0 == 0x80 {
                octets[2..4].fill(0);
            }
            Some(Ipv6Addr::from(octets).to_string())
        }
        _ => None,
    }
}

fn parse_link_address(address: &[u8]) -> Option<String> {
    if *address.get(1)? as i32 != libc::AF_LINK || *address.get(6)? != 6 {
        return None;
    }
    let start = 8 + *address.get(5)? as usize;
    Some(
        address
            .get(start..start + 6)?
            .iter()
            .map(|octet| format!("{octet:02x}"))
            .collect::<Vec<_>>()
            .join(":"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(mac: [u8; 6]) -> Vec<u8> {
        let mut bytes = vec![0; size_of::<libc::rt_msghdr>() + 16 + 20];
        let length = bytes.len() as u16;
        bytes[..2].copy_from_slice(&length.to_ne_bytes());
        bytes[std::mem::offset_of!(libc::rt_msghdr, rtm_version)] = libc::RTM_VERSION as u8;
        let offset = std::mem::offset_of!(libc::rt_msghdr, rtm_flags);
        bytes[offset..offset + 4].copy_from_slice(&libc::RTF_LLINFO.to_ne_bytes());
        let offset = std::mem::offset_of!(libc::rt_msghdr, rtm_addrs);
        let addresses = (1_i32 << libc::RTAX_DST) | (1 << libc::RTAX_GATEWAY);
        bytes[offset..offset + 4].copy_from_slice(&addresses.to_ne_bytes());
        let start = size_of::<libc::rt_msghdr>();
        bytes[start] = 16;
        bytes[start + 1] = libc::AF_INET as u8;
        bytes[start + 4..start + 8].copy_from_slice(&[192, 168, 1, 42]);
        let start = start + 16;
        bytes[start] = 20;
        bytes[start + 1] = libc::AF_LINK as u8;
        bytes[start + 5] = 3;
        bytes[start + 6] = 6;
        bytes[start + 8..start + 11].copy_from_slice(b"en0");
        bytes[start + 11..start + 17].copy_from_slice(&mac);
        bytes
    }

    #[test]
    fn reads_mac_after_interface_name_without_confusing_it_with_ip() {
        let rows = parse_table(&message([2, 0xaa, 0xbb, 0xcc, 0xdd, 1]));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].ip_address, "192.168.1.42");
        assert_eq!(
            rows[0].link_layer_address,
            ["02", "aa", "bb", "cc", "dd", "01"].join(":")
        );
    }

    #[test]
    fn rejects_hidden_incomplete_and_truncated_hardware_addresses() {
        assert!(parse_table(&message([2, 0, 0, 0, 0, 0])).is_empty());
        assert!(parse_table(&message([0; 6])).is_empty());
        let valid = message([2, 0xaa, 0xbb, 0xcc, 0xdd, 1]);
        for length in 0..valid.len() {
            assert!(parse_table(&valid[..length]).is_empty());
        }
        let mut incomplete = valid.clone();
        incomplete[size_of::<libc::rt_msghdr>() + 16 + 6] = 0;
        assert!(parse_table(&incomplete).is_empty());
        let mut oversized = valid;
        oversized[size_of::<libc::rt_msghdr>()] = 255;
        assert!(parse_table(&oversized).is_empty());
    }

    #[test]
    fn strips_embedded_ipv6_scope_without_changing_global_addresses() {
        let mut address = [0_u8; 28];
        address[0] = 28;
        address[1] = libc::AF_INET6 as u8;
        address[8..24].copy_from_slice(&"fe80:11::42".parse::<Ipv6Addr>().unwrap().octets());
        assert_eq!(parse_ip(&address).unwrap(), "fe80::42");
        address[8..24].copy_from_slice(&"2001:db8::42".parse::<Ipv6Addr>().unwrap().octets());
        assert_eq!(parse_ip(&address).unwrap(), "2001:db8::42");
    }

    #[test]
    fn route_cache_lifetime_does_not_establish_reachability() {
        let mut bytes = message([2, 0xaa, 0xbb, 0xcc, 0xdd, 1]);
        let expire = std::mem::offset_of!(libc::rt_msghdr, rtm_rmx)
            + std::mem::offset_of!(libc::rt_metrics, rmx_expire);
        bytes[expire..expire + 4].copy_from_slice(&1_i32.to_ne_bytes());
        assert_eq!(parse_table(&bytes)[0].state, "Cached");
        bytes[expire..expire + 4].copy_from_slice(&i32::MAX.to_ne_bytes());
        assert_eq!(parse_table(&bytes)[0].state, "Cached");
        bytes[std::mem::offset_of!(libc::rt_msghdr, rtm_version)] = 0;
        assert!(parse_table(&bytes).is_empty());
    }

    #[test]
    fn native_arp_requires_unexpired_receive_evidence() {
        let base = message([2, 0xaa, 0xbb, 0xcc, 0xdd, 1]);
        let mut bytes = vec![0; size_of::<ArpRouteHeader>()];
        bytes.extend_from_slice(&base[size_of::<libc::rt_msghdr>()..]);
        let len = bytes.len() as u16;
        bytes[..2].copy_from_slice(&len.to_ne_bytes());
        bytes[2] = libc::RTM_VERSION as u8;
        bytes[3] = RTM_GET_EXT;
        let flags = std::mem::offset_of!(ArpRouteHeader, flags);
        bytes[flags..flags + 4].copy_from_slice(&libc::RTF_LLINFO.to_ne_bytes());
        let addrs = std::mem::offset_of!(ArpRouteHeader, addrs);
        bytes[addrs..addrs + 4].copy_from_slice(&(libc::RTA_DST | libc::RTA_GATEWAY).to_ne_bytes());
        let send = std::mem::offset_of!(ArpRouteHeader, _send_expire);
        bytes[send..send + 8].copy_from_slice(&200_u64.to_ne_bytes());
        let refs = std::mem::offset_of!(ArpRouteHeader, refs);
        bytes[refs..refs + 4].copy_from_slice(&1_u32.to_ne_bytes());
        assert!(active_arp_row(&bytes, 100).is_none());
        let recv = std::mem::offset_of!(ArpRouteHeader, receive_expire);
        bytes[recv..recv + 8].copy_from_slice(&101_u64.to_ne_bytes());
        let row = active_arp_row(&bytes, 100).unwrap();
        assert_eq!(row.ip_address, "192.168.1.42");
        assert_eq!(row.link_layer_address, "02:aa:bb:cc:dd:01");
        assert!(active_arp_row(&bytes, 101).is_none());
        bytes[refs..refs + 4].fill(0);
        assert!(active_arp_row(&bytes, 100).is_none());
    }

    #[test]
    fn native_ndp_only_confirms_a_current_reachable_address() {
        let confirmed = |state, expiry| super::super::is_current_neighbor_state(classify_ndp(state, expiry, 100));
        assert!(confirmed(1, 101));
        for (state, expiry) in [(1, 100), (1, 0), (0, 101), (2, 101), (3, 101), (4, 101)] {
            assert!(!confirmed(state, expiry));
        }
    }

    #[test]
    fn walks_multiple_messages_and_rejects_zero_length_headers() {
        let mut bytes = message([2, 0xaa, 0xbb, 0xcc, 0xdd, 1]);
        bytes.extend(message([2, 0xaa, 0xbb, 0xcc, 0xdd, 2]));
        assert_eq!(parse_table(&bytes).len(), 2);
        bytes[..2].fill(0);
        assert!(parse_table(&bytes).is_empty());
    }
}
