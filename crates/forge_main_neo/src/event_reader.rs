use std::time::Duration;

use ratatui::crossterm::event::{self};
use tokio::sync::mpsc::Sender;

use crate::domain::Action;

pub struct EventReader {
    timeout: Duration,
}

impl EventReader {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    pub async fn init(&self, tx: Sender<anyhow::Result<Action>>) {
        let timeout = self.timeout;
        tokio::spawn(async move {
            while !tx.is_closed() {
                if event::poll(timeout).unwrap() && !tx.is_closed() {
                    let e = event::read().unwrap();
                    tx.send(Ok(e.into())).await.unwrap();
                }
            }
        });
    }
}