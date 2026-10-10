use std::sync::{Mutex, MutexGuard, Once};

static PROXY_STORE_LOCK: Mutex<()> = Mutex::new(());

pub fn proxy_store_lock() -> MutexGuard<'static, ()> {
    PROXY_STORE_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn init_logger() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_max_level(tracing::Level::DEBUG)
            .with_test_writer()
            .try_init();
    });
}
