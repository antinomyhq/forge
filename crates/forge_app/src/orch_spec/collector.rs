use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::sync::mpsc::Receiver;
use tokio::task::JoinHandle;

pub struct Collector<T> {
    accumulated: Arc<Mutex<Vec<T>>>,
    #[allow(unused)]
    handle: Arc<JoinHandle<()>>,
}

impl<T: Send + 'static> Collector<T> {
    pub fn new(mut rx: Receiver<T>, _limit: usize) -> Self {
        let accumulated = Arc::new(Mutex::new(Vec::new()));
        let accumulator_ref = accumulated.clone();

        let handle = Arc::new(tokio::spawn(async move {
            while let Some(item) = rx.recv().await {
                let mut acc = accumulator_ref.lock().await;
                acc.push(item);
                // Lock is released here, minimizing contention
            }
        }));

        Self { accumulated, handle }
    }

    pub async fn get_results(&self) -> Vec<T> {
        let mut acc = self.accumulated.lock().await;
        std::mem::take(&mut *acc)
    }
}
