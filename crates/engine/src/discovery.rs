//! Continuous mDNS discovery of AirPlay speakers.

use crate::error::EngineError;
use crate::speakers::{group_speakers, Speaker, Target};
use airplay_core::{Device, DeviceId};
use airplay_discovery::{BrowseEvent, Discovery, ServiceBrowser};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;
use tracing::{debug, info, warn};

/// Browses `_airplay._tcp` / `_raop._tcp` in the background and publishes the
/// current list of speakers.
pub struct SpeakerDiscovery {
    targets: Arc<RwLock<Vec<Target>>>,
    rx: watch::Receiver<Vec<Speaker>>,
    browser: Arc<ServiceBrowser>,
    task: JoinHandle<()>,
}

impl SpeakerDiscovery {
    /// Starts browsing. Must be called inside a Tokio runtime.
    pub async fn start() -> Result<Self, EngineError> {
        let browser =
            Arc::new(ServiceBrowser::new().map_err(|e| EngineError::Discovery(e.to_string()))?);
        let mut events = browser
            .browse()
            .await
            .map_err(|e| EngineError::Discovery(e.to_string()))?;
        let targets: Arc<RwLock<Vec<Target>>> = Arc::default();
        let (tx, rx) = watch::channel(Vec::new());

        let task_targets = Arc::clone(&targets);
        let task = tokio::spawn(async move {
            let mut devices: HashMap<DeviceId, Device> = HashMap::new();
            while let Some(event) = events.next().await {
                match event {
                    BrowseEvent::Added(d) | BrowseEvent::Updated(d) => {
                        debug!(name = %d.name, id = %d.id.to_mac_string(), "speaker advertised");
                        devices.insert(d.id.clone(), d);
                    }
                    BrowseEvent::Removed(id) => {
                        if let Some(d) = devices.remove(&id) {
                            info!(name = %d.name, "speaker went away");
                        }
                    }
                }
                let list: Vec<Device> = devices.values().cloned().collect();
                let grouped = group_speakers(&list);
                let speakers: Vec<Speaker> = grouped.iter().map(|t| t.speaker.clone()).collect();
                match task_targets.write() {
                    Ok(mut g) => *g = grouped,
                    Err(p) => *p.into_inner() = grouped,
                }
                tx.send_if_modified(|current| {
                    if *current == speakers {
                        false
                    } else {
                        *current = speakers;
                        true
                    }
                });
            }
            warn!("mDNS browse stream ended");
        });

        Ok(Self {
            targets,
            rx,
            browser,
            task,
        })
    }

    /// Receives the speaker list whenever it changes.
    pub fn subscribe(&self) -> watch::Receiver<Vec<Speaker>> {
        self.rx.clone()
    }

    pub fn speakers(&self) -> Vec<Speaker> {
        self.rx.borrow().clone()
    }

    /// Looks up the connectable target behind a speaker id.
    pub fn target(&self, speaker_id: &str) -> Option<Target> {
        let guard = self.targets.read().unwrap_or_else(|p| p.into_inner());
        guard.iter().find(|t| t.speaker.id == speaker_id).cloned()
    }

    /// Waits until at least one speaker is known or `timeout` elapses.
    pub async fn wait_for_any(&self, timeout: std::time::Duration) -> Vec<Speaker> {
        let mut rx = self.subscribe();
        let _ = tokio::time::timeout(timeout, rx.wait_for(|s| !s.is_empty())).await;
        self.speakers()
    }
}

impl Drop for SpeakerDiscovery {
    fn drop(&mut self) {
        self.task.abort();
        let browser = Arc::clone(&self.browser);
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move { browser.stop().await });
        }
    }
}
