use crate::events::AgentEvent;
use reqwest::Client;
use serde_json::json;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::Mutex;

pub struct TelemetryWorker {
    client: Client,
    project_id: String,
    dataset: String,
    table: String,
    watts: Arc<Mutex<f32>>,
    joules: Arc<Mutex<f32>>,
    rx_kbps: Arc<Mutex<f64>>,
    tx_kbps: Arc<Mutex<f64>>,
}

impl TelemetryWorker {
    pub fn new(project_id: &str, dataset: &str, table: &str) -> Self {
        Self {
            client: Client::new(),
            project_id: project_id.to_string(),
            dataset: dataset.to_string(),
            table: table.to_string(),
            watts: Arc::new(Mutex::new(0.0)),
            joules: Arc::new(Mutex::new(0.0)),
            rx_kbps: Arc::new(Mutex::new(0.0)),
            tx_kbps: Arc::new(Mutex::new(0.0)),
        }
    }

    fn get_token() -> String {
        if let Ok(token) = std::env::var("GCP_ACCESS_TOKEN") {
            return token;
        }
        let output = Command::new("gcloud")
            .args(["auth", "print-access-token"])
            .output();

        if let Ok(out) = output {
            if out.status.success() {
                return String::from_utf8_lossy(&out.stdout).trim().to_string();
            }
        }
        String::new()
    }

    pub async fn run(self, mut rx: broadcast::Receiver<AgentEvent>) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));

        loop {
            tokio::select! {
                Ok(event) = rx.recv() => {
                    match event {
                        AgentEvent::PowerUpdate { watts, joules } => {
                            let mut w = self.watts.lock().await;
                            *w = watts;
                            let mut j = self.joules.lock().await;
                            *j = joules;
                        }
                        AgentEvent::TelemetryUpdate { rx_kbps, tx_kbps } => {
                            let mut r = self.rx_kbps.lock().await;
                            *r = rx_kbps;
                            let mut t = self.tx_kbps.lock().await;
                            *t = tx_kbps;
                        }
                        _ => {}
                    }
                }
                _ = interval.tick() => {
                    let w = *self.watts.lock().await;
                    let j = *self.joules.lock().await;
                    let r = *self.rx_kbps.lock().await;
                    let t = *self.tx_kbps.lock().await;

                    // Only send if there's actual power usage
                    if w > 0.0 {
                        let token = Self::get_token();
                        if !token.is_empty() {
                            let url = format!(
                                "https://bigquery.googleapis.com/bigquery/v2/projects/{}/datasets/{}/tables/{}/insertAll",
                                self.project_id, self.dataset, self.table
                            );
                            // Ensure chrono format uses standard RFC3339 without subseconds for BQ compatibility
                            let now = std::time::SystemTime::now();
                            let datetime: chrono::DateTime<chrono::Utc> = now.into();
                            let timestamp = datetime.to_rfc3339();

                            let payload = json!({
                                "rows": [
                                    {
                                        "json": {
                                            "timestamp": timestamp,
                                            "watts": w,
                                            "joules": j,
                                            "rx_kbps": r,
                                            "tx_kbps": t
                                        }
                                    }
                                ]
                            });

                            let _ = self.client.post(&url)
                                .bearer_auth(&token)
                                .json(&payload)
                                .send()
                                .await;
                        }
                    }
                }
            }
        }
    }
}
