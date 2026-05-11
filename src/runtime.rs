use std::sync::atomic::{AtomicBool, Ordering};

static OFFLINE_MODE: AtomicBool = AtomicBool::new(false);
static REFRESH_MODE: AtomicBool = AtomicBool::new(false);

pub fn set_offline(value: bool) {
    OFFLINE_MODE.store(value, Ordering::Relaxed);
}

pub fn offline() -> bool {
    OFFLINE_MODE.load(Ordering::Relaxed)
}

pub fn set_refresh(value: bool) {
    REFRESH_MODE.store(value, Ordering::Relaxed);
}

pub fn refresh() -> bool {
    REFRESH_MODE.load(Ordering::Relaxed)
}
