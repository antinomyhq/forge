use tokio::sync::broadcast;

/// Application-wide signals that can be used to coordinate operations
#[derive(Debug, Clone)]
pub enum AppSignal {
    /// Signals cancellation of current operation
    Cancel,
    /// Signals application exit
    Exit,
}

/// A centralized signal manager for handling application-wide signals
pub struct SignalManager {
    tx: broadcast::Sender<AppSignal>,
}

impl SignalManager {
    /// Create a new signal manager
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(16);
        Self { tx }
    }

    /// Subscribe to signals
    pub fn subscribe(&self) -> broadcast::Receiver<AppSignal> {
        self.tx.subscribe()
    }

    /// Send a cancel signal
    pub fn cancel(&self) {
        let _ = self.tx.send(AppSignal::Cancel);
    }

    /// Send an exit signal
    pub fn exit(&self) {
        let _ = self.tx.send(AppSignal::Exit);
    }
}

impl Default for SignalManager {
    fn default() -> Self {
        Self::new()
    }
}
