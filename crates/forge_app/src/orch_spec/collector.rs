use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::sync::mpsc::Receiver;
use tokio::task::JoinHandle;

struct State<T> {
    results: Vec<T>,
    rx: Receiver<T>,
}

pub struct Collector<T> {
    state: Arc<Mutex<State<T>>>,
    #[allow(unused)]
    handle: Arc<JoinHandle<()>>,
}

impl<T: Send + Sync + 'static> Collector<T> {
    pub fn new(rx: Receiver<T>, limit: usize) -> Self {
        let state = Arc::new(Mutex::new(State { results: Default::default(), rx }));
        let update_state = state.clone();
        let handle = Arc::new(tokio::spawn(async move {
            let mut state = update_state.lock().await;
            let mut buffer = Vec::new();
            state.rx.recv_many(&mut buffer, limit).await;
            state.results.extend(buffer);
        }));

        Self { state, handle }
    }

    pub async fn get_results(&self) -> Vec<T> {
        let mut state = self.state.lock().await;

        std::mem::take(&mut state.results)
    }
}
