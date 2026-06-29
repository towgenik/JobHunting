//! HTTP route handlers — HTML responses for the web UI.
//!
//! Each handler group lives in its own file: jobs, profile, wiki, settings.

pub mod forms;
pub mod jobs;
pub mod profile;
pub mod profile_print;
pub mod settings;
pub mod wiki;

/// Drop guard for AtomicBool — releases on panic.
pub struct BoolGuard(pub(crate) std::sync::Arc<std::sync::atomic::AtomicBool>);
impl Drop for BoolGuard {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}
