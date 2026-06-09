use std::sync::Once;

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
