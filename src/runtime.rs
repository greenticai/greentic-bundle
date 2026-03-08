use std::sync::atomic::{AtomicBool, Ordering};

static OFFLINE_MODE: AtomicBool = AtomicBool::new(false);

pub fn set_offline(value: bool) {
    OFFLINE_MODE.store(value, Ordering::Relaxed);
}

pub fn offline() -> bool {
    OFFLINE_MODE.load(Ordering::Relaxed)
}
