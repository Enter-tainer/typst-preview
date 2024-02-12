use std::borrow::Cow;

use await_tree::{Config, Registry};
use once_cell::sync::Lazy;
use tokio::sync::Mutex;
pub static REGISTRY: Lazy<Mutex<Registry<Cow<'static, str>>>> =
    Lazy::new(|| Mutex::new(Registry::new(Config::default())));
