use once_cell::sync::OnceCell;
use tokio::runtime::Runtime;

// Global tokio runtime for async I/O operations
// Shared between plugin downloads, timers, and action completion tracking
static TOKIO_RUNTIME: OnceCell<Runtime> = OnceCell::new();

pub fn get_tokio_runtime() -> &'static Runtime {
    TOKIO_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .thread_name("async-runtime")
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime")
    })
}
