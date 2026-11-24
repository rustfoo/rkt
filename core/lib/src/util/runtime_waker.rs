use std::io;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use tokio::runtime::{Handle, RuntimeFlavor};
use tokio::task::JoinHandle;

pub(crate) struct RuntimeWaker {
    stop: mpsc::Sender<()>,
    thread: Option<thread::JoinHandle<()>>,
}

impl RuntimeWaker {
    pub(crate) fn start(interval: Duration) -> io::Result<Option<Self>> {
        let handle = Handle::current();
        if interval.is_zero() || handle.runtime_flavor() != RuntimeFlavor::MultiThread {
            return Ok(None);
        }

        let (stop, stopped) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("rkt-runtime-waker".into())
            .spawn(move || {
                let mut wake: Option<JoinHandle<()>> = None;
                while stopped.recv_timeout(interval) == Err(RecvTimeoutError::Timeout) {
                    // Spawning from outside Tokio wakes an idle worker. Keep at
                    // most one task queued even when every worker is blocked.
                    if wake.as_ref().is_none_or(JoinHandle::is_finished) {
                        wake = Some(handle.spawn(async {}));
                    }
                }

                if let Some(wake) = wake {
                    wake.abort();
                }
            })?;

        Ok(Some(Self {
            stop,
            thread: Some(thread),
        }))
    }
}

impl Drop for RuntimeWaker {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_disabled_and_current_thread_runtimes() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let _guard = runtime.enter();
        assert!(RuntimeWaker::start(Duration::from_millis(25))
            .unwrap()
            .is_none());

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .build()
            .unwrap();
        let _guard = runtime.enter();
        assert!(RuntimeWaker::start(Duration::ZERO).unwrap().is_none());
    }

    #[test]
    fn stops_without_waiting_for_the_next_wake() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .build()
            .unwrap();
        let _guard = runtime.enter();
        let waker = RuntimeWaker::start(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        let start = std::time::Instant::now();
        drop(waker);
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn timer_progresses_while_a_worker_is_blocked() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_time()
            .build()
            .unwrap();
        let _guard = runtime.enter();
        let (blocked, started) = mpsc::channel();
        let (release, hold) = mpsc::channel();
        let (ready, done) = mpsc::channel();

        runtime.spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            blocked.send(()).unwrap();
            let _ = hold.recv_timeout(Duration::from_secs(5));
        });
        runtime.spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let _ = ready.send(());
        });

        started.recv_timeout(Duration::from_secs(2)).unwrap();
        let waker = RuntimeWaker::start(Duration::from_millis(25))
            .unwrap()
            .unwrap();
        let result = done.recv_timeout(Duration::from_secs(2));
        let _ = release.send(());
        drop(waker);
        result.expect("timer should finish while the other worker is blocked");
    }
}
