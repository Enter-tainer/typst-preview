use std::sync::Arc;

use tokio::sync::RwLock;

#[derive(Debug)]
pub struct PublisherImpl<T> {
    notifier: tokio::sync::Notify,
    data: RwLock<Option<Arc<T>>>,
}

impl<T> PublisherImpl<T> {
    pub fn new() -> Self {
        Self {
            notifier: tokio::sync::Notify::new(),
            data: None.into(),
        }
    }

    #[allow(dead_code)]
    pub fn with_data(data: Arc<T>) -> Self {
        Self {
            notifier: tokio::sync::Notify::new(),
            data: Some(data).into(),
        }
    }

    pub async fn wait(&self) -> Arc<T> {
        self.notifier.notified().await;
        let mut data = self.data.read().await.clone();
        while data.is_none() {
            self.notifier.notified().await;
            data = self.data.read().await.clone();
        }
        data.unwrap()
    }

    pub async fn latest(&self) -> Option<Arc<T>> {
        self.data.read().await.clone()
    }

    pub async fn publish(&self, data: Arc<T>) {
        self.data.write().await.clone_from(&Some(data));
        self.notifier.notify_waiters();
    }
}

pub type Publisher<T> = Arc<PublisherImpl<T>>;
