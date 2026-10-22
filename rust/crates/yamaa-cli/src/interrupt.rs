//! Defer Ctrl+C to the facade's cancellation checkpoints, then preserve the
//! process's native interrupt termination after cleanup (D46, D74).

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

use yamaa::CancellationToken;

// Signal/console handlers only set this flag. They never allocate, lock,
// invoke the engine, or terminate in the middle of publication.
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

pub(super) fn install() -> io::Result<CancellationToken> {
    platform::install()?;
    Ok(CancellationToken::with_probe(|| {
        INTERRUPTED.load(Ordering::SeqCst)
    }))
}

/// Restore native handling before checking the flag, so a later interrupt
/// also terminates normally instead of being swallowed during process exit.
pub(super) fn finish() {
    platform::restore();
    if INTERRUPTED.load(Ordering::SeqCst) {
        platform::terminate();
    }
}

#[cfg(unix)]
mod platform {
    use super::*;

    extern "C" fn receive(_: libc::c_int) {
        INTERRUPTED.store(true, Ordering::SeqCst);
    }

    pub(super) fn install() -> io::Result<()> {
        // SAFETY: the zeroed sigaction is initialized before registration;
        // its handler has the required C ABI and touches only an atomic.
        // SA_RESTART lets an in-flight read finish before the engine polls.
        let result = unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = receive as *const () as usize;
            action.sa_flags = libc::SA_RESTART;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(libc::SIGINT, &action, std::ptr::null_mut())
        };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub(super) fn restore() {
        // SAFETY: SIGINT and SIG_DFL are the platform's valid constants.
        unsafe { libc::signal(libc::SIGINT, libc::SIG_DFL) };
    }

    pub(super) fn terminate() -> ! {
        // SAFETY: the default disposition is restored, and no engine frame
        // remains to be interrupted. Preserve WIFSIGNALED/SIGINT on POSIX.
        unsafe { libc::raise(libc::SIGINT) };
        std::process::exit(130)
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use windows_sys::core::BOOL;
    use windows_sys::Win32::Foundation::STATUS_CONTROL_C_EXIT;
    use windows_sys::Win32::System::Console::{
        SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_C_EVENT,
    };

    unsafe extern "system" fn receive(event: u32) -> BOOL {
        match event {
            CTRL_C_EVENT | CTRL_BREAK_EVENT => {
                INTERRUPTED.store(true, Ordering::SeqCst);
                1
            }
            _ => 0,
        }
    }

    pub(super) fn install() -> io::Result<()> {
        // SAFETY: receive has the console handler ABI and uses only an
        // atomic; it lives for the entire process.
        if unsafe { SetConsoleCtrlHandler(Some(receive), 1) } != 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub(super) fn restore() {
        // SAFETY: remove the same static handler that install registered.
        unsafe { SetConsoleCtrlHandler(Some(receive), 0) };
    }

    pub(super) fn terminate() -> ! {
        // The default Windows console handler exits with this status.
        std::process::exit(STATUS_CONTROL_C_EXIT)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// D46: console interrupts cancel cooperatively; console close and
        /// shutdown events keep the operating system's existing handling.
        #[test]
        fn console_interrupts_cancel_before_native_termination() {
            const CHILD: &str = "YAMAA_CLI_CONSOLE_INTERRUPT_TEST";
            if let Some(event) = std::env::var_os(CHILD) {
                let event: u32 = event.to_str().unwrap().parse().unwrap();
                let token = super::super::install().unwrap();
                // SAFETY: invoke the registered callback with an authored
                // console event, without signalling the test runner's console.
                let handled = unsafe { receive(event) };
                assert_eq!(
                    (handled, token.is_cancelled()),
                    (1, true),
                    "console event {event}"
                );
                super::super::finish();
                panic!("an interrupted command must terminate");
            }
            for event in [CTRL_C_EVENT, CTRL_BREAK_EVENT] {
                let output = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "interrupt::platform::tests::console_interrupts_cancel_before_native_termination",
                    ])
                    .env(CHILD, event.to_string())
                    .output()
                    .unwrap();
                assert_eq!(
                    output.status.code(),
                    Some(STATUS_CONTROL_C_EXIT),
                    "{event}: {output:?}"
                );
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// D46: the real POSIX handler reaches the token, and cleanup precedes
    /// native signal termination. Isolate the process-wide handler from
    /// the test harness and the other tests; no timing or sleep is needed.
    #[test]
    fn ctrl_c_cancels_before_native_signal_termination() {
        const CHILD: &str = "YAMAA_CLI_INTERRUPT_TEST";
        if std::env::var_os(CHILD).is_some() {
            let token = install().unwrap();
            let shared = token.clone();
            // SAFETY: this child installed its handler before raising SIGINT.
            assert_eq!(unsafe { libc::raise(libc::SIGINT) }, 0);
            assert_eq!(
                (token.is_cancelled(), shared.is_cancelled()),
                (true, true),
                "SIGINT cancels every token sharing the command's flag"
            );
            finish();
            panic!("an interrupted command must terminate by SIGINT");
        }
        use std::os::unix::process::ExitStatusExt;
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "interrupt::tests::ctrl_c_cancels_before_native_signal_termination",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert_eq!(output.status.signal(), Some(libc::SIGINT), "{output:?}");
    }
}
