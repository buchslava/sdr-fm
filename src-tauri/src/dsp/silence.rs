//! Briefly redirect process-wide stderr to `/dev/null` while a noisy native
//! call runs. Suppresses libSoapySDR / librtlsdr output during device open.

#[cfg(unix)]
use std::fs::File;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;

/// RAII guard that restores stderr on drop.
#[cfg(unix)]
pub(super) struct SilencedStderr {
    backup: i32,
    _devnull: File,
}

#[cfg(unix)]
impl SilencedStderr {
    pub(super) fn new() -> Self {
        unsafe {
            let devnull = File::open("/dev/null").expect("/dev/null missing");
            let null_fd = devnull.as_raw_fd();
            let backup = libc::dup(2);
            libc::dup2(null_fd, 2);
            Self {
                backup,
                _devnull: devnull,
            }
        }
    }
}

#[cfg(unix)]
impl Drop for SilencedStderr {
    fn drop(&mut self) {
        unsafe {
            libc::dup2(self.backup, 2);
            libc::close(self.backup);
        }
    }
}

#[cfg(unix)]
pub(super) fn silenced<R>(f: impl FnOnce() -> R) -> R {
    let _guard = SilencedStderr::new();
    f()
}

#[cfg(not(unix))]
pub(super) fn silenced<R>(f: impl FnOnce() -> R) -> R {
    f()
}

pub(super) async fn silence_during_async<F: std::future::Future>(fut: F) -> F::Output {
    #[cfg(unix)]
    {
        let _guard = SilencedStderr::new();
        fut.await
    }
    #[cfg(not(unix))]
    {
        fut.await
    }
}
