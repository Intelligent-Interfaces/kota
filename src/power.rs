use crate::events::AgentEvent;
use macpow::metrics::Sampler;
use std::time::Duration;
use tokio::sync::broadcast;

pub async fn start_power_monitor(
    tx: broadcast::Sender<AgentEvent>,
    mut rx: broadcast::Receiver<AgentEvent>,
) {
    let mut total_joules: f64 = 0.0;
    let mut is_busy = false;
    let mut last_was_busy = false;

    // Spawn the sampler on a blocking thread because it might block,
    // though Sampler::new spawns its own background threads.
    let sampler = tokio::task::spawn_blocking(|| Sampler::new(500))
        .await
        .unwrap();

    let mut interval = tokio::time::interval(Duration::from_millis(500));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Reset joules when starting a new busy cycle
                if is_busy && !last_was_busy {
                    total_joules = 0.0;
                }

                let metrics = sampler.snapshot();
                let watts = metrics.soc.total_w as f64; // Use SoC total watts

                // Accumulate joules (watts * 0.5 seconds)
                if is_busy {
                    total_joules += watts * 0.5;
                }

                // Always broadcast power so the UI can update live
                let _ = tx.send(AgentEvent::PowerUpdate {
                    watts: watts as f32,
                    joules: total_joules as f32,
                });

                last_was_busy = is_busy;
            }
            Ok(event) = rx.recv() => {
                match event {
                    AgentEvent::StepStarted { .. } => is_busy = true,
                    AgentEvent::Done { .. } | AgentEvent::CommandFinished | AgentEvent::Error { .. } => is_busy = false,
                    _ => {}
                }
            }
        }
    }
}
