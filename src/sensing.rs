use crate::events::AgentEvent;
use sysinfo::Networks;
use tokio::sync::broadcast;
use tokio::time::{sleep, Duration};

pub async fn run_telemetry_loop(tx: broadcast::Sender<AgentEvent>) {
    let mut networks = Networks::new_with_refreshed_list();

    loop {
        networks.refresh(true);

        let mut total_rx = 0;
        let mut total_tx = 0;

        for (_interface_name, data) in &networks {
            total_rx += data.received();
            total_tx += data.transmitted();
        }

        let event = process_bandwidth(total_rx, total_tx);
        let _ = tx.send(event);

        sleep(Duration::from_millis(1000)).await;
    }
}

/// Pure logic to process raw interface bytes and emit events.
pub fn process_bandwidth(rx: u64, tx: u64) -> AgentEvent {
    // Convert bytes per second to kbps (assuming 1s interval)
    let rx_kbps = (rx as f64) / 1024.0 * 8.0;
    let tx_kbps = (tx as f64) / 1024.0 * 8.0;

    if tx_kbps > 500_000.0 {
        AgentEvent::NetworkThreatDetected {
            severity: "CRITICAL".to_string(),
            description: format!(
                "Massive outbound bandwidth spike detected: {:.2} Kbps",
                tx_kbps
            ),
        }
    } else {
        AgentEvent::TelemetryUpdate { rx_kbps, tx_kbps }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_bandwidth_normal() {
        // Low bytes transferred
        let event = process_bandwidth(1024, 2048);
        match event {
            AgentEvent::TelemetryUpdate { rx_kbps, tx_kbps } => {
                assert!(rx_kbps > 0.0);
                assert!(tx_kbps > 0.0);
                // 1024 bytes = 1 KB = 8 Kbps
                assert_eq!(rx_kbps, 8.0);
                // 2048 bytes = 2 KB = 16 Kbps
                assert_eq!(tx_kbps, 16.0);
            }
            _ => panic!("Expected TelemetryUpdate event"),
        }
    }

    #[test]
    fn test_process_bandwidth_threat() {
        // 65 megabytes = 507,812.5 kbps (triggers threat > 500,000 kbps)
        let event = process_bandwidth(1024, 65_000_000);
        match event {
            AgentEvent::NetworkThreatDetected {
                severity,
                description,
            } => {
                assert_eq!(severity, "CRITICAL");
                assert!(description.contains("Massive outbound bandwidth spike"));
            }
            _ => panic!("Expected NetworkThreatDetected event"),
        }
    }
}
