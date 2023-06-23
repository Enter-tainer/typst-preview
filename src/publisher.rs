use std::sync::Arc;

use tokio::sync::RwLock;

#[derive(Debug)]
pub struct PublisherImpl<T> {
    notifier: tokio::sync::Notify,
    data: RwLock<Arc<T>>,
}

impl<T> PublisherImpl<T> {
    pub fn new(data: T) -> Self {
        Self {
            notifier: tokio::sync::Notify::new(),
            data: Arc::new(data).into(),
        }
    }

    pub async fn wait(&self) -> Arc<T> {
        self.notifier.notified().await;
        self.data.read().await.clone()
    }
    
    pub async fn latest(&self) -> Arc<T> {
      self.data.read().await.clone()
    }

    pub async fn publish(&self, data: Arc<T>) {
        self.data.write().await.clone_from(&data);
        self.notifier.notify_waiters();
    }
}

pub type Publisher<T> = Arc<PublisherImpl<T>>;
