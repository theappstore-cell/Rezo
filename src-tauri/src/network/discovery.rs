use std::net::IpAddr;

use anyhow::Result;
use mdns_sd::{ServiceDaemon, ServiceEvent};

pub const SERVICE_TYPE: &str = "_rezo._udp.local.";

#[derive(Clone, Debug)]
pub struct NativeAnnounce {
    pub id: String,
    pub name: String,
    pub ip: IpAddr,
    pub port: u16,
}

pub fn browse(on_found: impl Fn(NativeAnnounce) + Send + 'static) -> Result<ServiceDaemon> {
    let daemon = ServiceDaemon::new()?;
    let lan = super::lan_ipv4_addrs();
    if !lan.is_empty() {
        let _ = daemon.disable_interface(mdns_sd::IfKind::All);
        for ip in lan {
            let _ = daemon.enable_interface(mdns_sd::IfKind::Addr(std::net::IpAddr::V4(ip)));
        }
    }
    let rx = daemon.browse(SERVICE_TYPE)?;
    tauri::async_runtime::spawn(async move {
        while let Ok(ev) = rx.recv_async().await {
            if let ServiceEvent::ServiceResolved(info) = ev {
                let addrs = info.get_addresses();
                let ip = addrs.iter().find(|a| a.is_ipv4()).or_else(|| addrs.iter().next()).map(|a| a.to_ip_addr());
                let Some(ip) = ip else { continue };
                let fullname = info.get_fullname().to_string();
                let instance = fullname.split('.').next().unwrap_or("Récepteur").to_string();
                on_found(NativeAnnounce {
                    id: info.get_property_val_str("id").map(str::to_string).unwrap_or_else(|| instance.clone()),
                    name: info.get_property_val_str("name").map(str::to_string).unwrap_or(instance),
                    ip,
                    port: info.get_port(),
                });
            }
        }
    });
    Ok(daemon)
}
