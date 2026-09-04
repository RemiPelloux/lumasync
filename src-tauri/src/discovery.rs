use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use crate::models::BridgeInfo;

pub fn discover() -> Result<Vec<BridgeInfo>, String> {
    let daemon = ServiceDaemon::new()
        .map_err(|error| format!("La détection mDNS n’a pas pu démarrer : {error}"))?;
    let receiver = daemon
        .browse("_hue._tcp.local.")
        .map_err(|error| format!("La recherche des ponts Hue a échoué : {error}"))?;
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut bridges = BTreeMap::<String, BridgeInfo>::new();

    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(remaining.min(Duration::from_millis(450))) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                let host = info
                    .get_addresses()
                    .iter()
                    .find(|address| address.is_ipv4())
                    .or_else(|| info.get_addresses().iter().next())
                    .map(ToString::to_string);
                if let Some(host) = host {
                    let bridge_id = info
                        .get_property_val_str("bridgeid")
                        .unwrap_or_default()
                        .to_owned();
                    let key = if bridge_id.is_empty() {
                        host.clone()
                    } else {
                        bridge_id.clone()
                    };
                    bridges.insert(
                        key,
                        BridgeInfo {
                            id: bridge_id,
                            host,
                            name: info.get_hostname().trim_end_matches('.').to_owned(),
                            port: info.get_port(),
                        },
                    );
                }
            }
            Ok(_) => {}
            Err(_) => {}
        }
    }

    let _ = daemon.stop_browse("_hue._tcp.local.");
    let _ = daemon.shutdown();
    Ok(bridges.into_values().collect())
}
