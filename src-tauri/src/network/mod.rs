pub mod discovery;
pub mod http_server;
pub mod sonos_upnp;
pub mod udp_streamer;
pub mod web_receiver;
pub mod xml;

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};

use parking_lot::Mutex;
use rtrb::Consumer;

pub struct PcmTap {
    pub cons: Mutex<Consumer<f32>>,
    pub connected: AtomicBool,
    pub client_buf_ms: AtomicU32,
    pub gen: AtomicU64,
    // Receiver-side volume (0-100) for browser receivers.
    pub volume: AtomicU32,
    // Highest byte offset of the HTTP stream served so far: a Range request beyond it is a probe, not a resume.
    pub position: AtomicU64,
}

impl PcmTap {
    pub fn new(cons: Consumer<f32>, volume: u32) -> Self {
        Self {
            cons: Mutex::new(cons),
            connected: AtomicBool::new(false),
            client_buf_ms: AtomicU32::new(0),
            gen: AtomicU64::new(0),
            volume: AtomicU32::new(volume.min(100)),
            position: AtomicU64::new(0),
        }
    }

    pub fn drain(&self) {
        let mut c = self.cons.lock();
        let n = c.slots();
        if n > 0 {
            if let Ok(chunk) = c.read_chunk(n) {
                chunk.commit_all();
            }
        }
    }

    pub fn read_into(&self, out: &mut Vec<f32>, max: usize) -> usize {
        let mut c = self.cons.lock();
        let n = (c.slots().min(max)) & !1;
        if n == 0 {
            return 0;
        }
        if let Ok(chunk) = c.read_chunk(n) {
            let (a, b) = chunk.as_slices();
            out.extend_from_slice(a);
            out.extend_from_slice(b);
            chunk.commit_all();
            return n;
        }
        0
    }

    pub fn available(&self) -> usize {
        self.cons.lock().slots()
    }
}

pub static TAP_NOTIFY: tokio::sync::Notify = tokio::sync::Notify::const_new();

pub fn spawn_tap_pump() -> std::thread::Thread {
    let h = std::thread::Builder::new()
        .name("rezo-tap-pump".into())
        .spawn(|| loop {
            std::thread::park();
            TAP_NOTIFY.notify_waiters();
        })
        .expect("fil de relais des flux");
    h.thread().clone()
}

pub async fn wait_for_samples(tap: &PcmTap, min_samples: usize, max_wait: std::time::Duration) {
    let notified = TAP_NOTIFY.notified();
    tokio::pin!(notified);
    notified.as_mut().enable();
    if tap.available() >= min_samples {
        return;
    }
    let _ = tokio::time::timeout(max_wait, notified).await;
}

pub fn local_ip_for(peer: &str) -> Option<std::net::IpAddr> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect((peer, 1900)).ok()?;
    s.local_addr().ok().map(|a| a.ip())
}

fn is_virtual_adapter(name: &str) -> bool {
    const MARKERS: [&str; 17] = [
        "virtualbox", "vmware", "vethernet", "hyper-v", "wsl", "docker", "npcap", "loopback", "tap-",
        "tap ", "wireguard", "mullvad", "tailscale", "zerotier", "openvpn", "nordlynx", "vpn",
    ];
    let n = name.to_lowercase();
    MARKERS.iter().any(|m| n.contains(m))
}

pub fn lan_ipv4_addrs() -> Vec<std::net::Ipv4Addr> {
    #[cfg(windows)]
    {
        let mut c: Vec<(std::net::Ipv4Addr, bool)> = win_adapters::candidates()
            .into_iter()
            .filter(|a| a.up && !is_virtual_adapter(&a.name) && !is_virtual_adapter(&a.description))
            .filter(|a| a.ip.is_private() && !a.ip.is_link_local())
            .map(|a| (a.ip, a.gateway))
            .collect();
        if !c.is_empty() {
            c.sort_by_key(|(ip, gw)| (!*gw, rank(ip)));
            let mut v: Vec<std::net::Ipv4Addr> = c.into_iter().map(|(ip, _)| ip).collect();
            v.dedup();
            return v;
        }
    }
    let mut v: Vec<std::net::Ipv4Addr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter(|i| !is_virtual_adapter(&i.name))
        .filter_map(|i| match i.ip() {
            std::net::IpAddr::V4(a) if !a.is_link_local() && a.is_private() => Some(a),
            _ => None,
        })
        .collect();
    v.sort_by_key(rank);
    v.dedup();
    v
}

fn rank(a: &std::net::Ipv4Addr) -> u8 {
    match a.octets() {
        [192, 168, ..] => 0,
        [10, ..] => 1,
        _ => 2,
    }
}

#[cfg(windows)]
mod win_adapters {
    use std::net::Ipv4Addr;

    use windows::Win32::Foundation::ERROR_BUFFER_OVERFLOW;
    use windows::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER,
        GAA_FLAG_SKIP_MULTICAST, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
    use windows::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};

    pub struct Adapter {
        pub ip: Ipv4Addr,
        pub name: String,
        pub description: String,
        pub gateway: bool,
        pub up: bool,
    }

    pub fn candidates() -> Vec<Adapter> {
        let flags = GAA_FLAG_INCLUDE_GATEWAYS | GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
        let mut size: u32 = 16 * 1024;
        let mut buf: Vec<u64>;
        let mut tries = 0;
        unsafe {
            loop {
                buf = vec![0u64; (size as usize + 7) / 8];
                let r = GetAdaptersAddresses(AF_INET.0 as u32, flags, None, Some(buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH), &mut size);
                if r == ERROR_BUFFER_OVERFLOW.0 && tries < 3 {
                    tries += 1;
                    continue;
                }
                if r != 0 {
                    return vec![];
                }
                break;
            }
            let mut out = vec![];
            let mut p = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
            while !p.is_null() {
                let a = &*p;
                let name = a.FriendlyName.to_string().unwrap_or_default();
                let description = a.Description.to_string().unwrap_or_default();
                let gateway = !a.FirstGatewayAddress.is_null();
                let up = a.OperStatus == IfOperStatusUp;
                let mut u = a.FirstUnicastAddress;
                while !u.is_null() {
                    let sa = (*u).Address.lpSockaddr;
                    if !sa.is_null() && (*sa).sa_family == AF_INET {
                        let sin = &*(sa as *const SOCKADDR_IN);
                        let ip = Ipv4Addr::from(u32::from_be(sin.sin_addr.S_un.S_addr));
                        out.push(Adapter { ip, name: name.clone(), description: description.clone(), gateway, up });
                    }
                    u = (*u).Next;
                }
                p = a.Next;
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn virtual_adapters_are_recognised() {
        assert!(super::is_virtual_adapter("VirtualBox Host-Only Network"));
        assert!(super::is_virtual_adapter("Mullvad"));
        assert!(super::is_virtual_adapter("vEthernet (WSL)"));
        assert!(!super::is_virtual_adapter("Ethernet"));
        assert!(!super::is_virtual_adapter("Wi-Fi"));
        #[cfg(windows)]
        for a in super::win_adapters::candidates() {
            println!("carte : {:<14} {:<16} {:<45} passerelle={} active={}", a.name, a.ip.to_string(), a.description, a.gateway, a.up);
        }
        println!("retenues : {:?}", super::lan_ipv4_addrs());
    }
}
