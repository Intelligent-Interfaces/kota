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

        // Convert bytes per second (refresh rate is roughly 1s due to sleep) to kbps
        let rx_kbps = (total_rx as f64) / 1024.0 * 8.0;
        let tx_kbps = (total_tx as f64) / 1024.0 * 8.0;

        if tx_kbps > 500_000.0 {
            let _ = tx.send(AgentEvent::NetworkThreatDetected {
                severity: "CRITICAL".to_string(),
                description: format!(
                    "Massive outbound bandwidth spike detected: {:.2} Kbps",
                    tx_kbps
                ),
            });
        } else {
            let _ = tx.send(AgentEvent::TelemetryUpdate { rx_kbps, tx_kbps });
        }

        sleep(Duration::from_millis(1000)).await;
    }
}
