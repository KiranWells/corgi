/// Debouncer for events
///
/// The debouncer will only return true once the wait time has passed,
/// and will return false until triggered again.
///
/// # Usage
///
/// ```
/// use corgi::types::Debouncer;
///
/// let now = std::time::Instant::now();
/// let mut debouncer = Debouncer::new(std::time::Duration::from_millis(100));
///
/// // Trigger the debouncer
/// debouncer.trigger();
///
/// // Poll the debouncer
/// // This will return false until 100ms have passed
/// while !debouncer.poll() {
///    // sleep for 100ms
///    std::thread::sleep(std::time::Duration::from_millis(10));
/// }
/// // The debouncer can now be triggered again
/// assert!(now.elapsed() >= std::time::Duration::from_millis(100));
///
/// // Reset the debouncer
/// debouncer.reset();
/// assert!(!debouncer.poll());
/// ```
#[derive(Debug)]
pub struct Debouncer {
    pub wait_time: std::time::Duration,
    last_triggered: Option<std::time::Instant>,
}

impl Debouncer {
    /// Create a new debouncer with the given wait time
    pub fn new(wait: std::time::Duration) -> Self {
        Self {
            wait_time: wait,
            last_triggered: None,
        }
    }

    /// Trigger the debouncer. This will reset the timer.
    pub fn trigger(&mut self) {
        self.last_triggered = Some(std::time::Instant::now());
    }

    /// Poll the debouncer. This will return true if the wait time has passed,
    /// and will only return true once. It will return false until triggered again,
    /// and the wait time has passed.
    pub fn poll(&mut self) -> bool {
        if let Some(v) = self.last_triggered {
            let now = std::time::Instant::now();
            if now - v >= self.wait_time {
                self.last_triggered = None;
                return true;
            }
        }
        false
    }

    /// Reset the debouncer. This will reset the timer, requiring the debouncer
    /// to be triggered again before it will return true.
    pub fn reset(&mut self) {
        self.last_triggered = None;
    }

    /// Returns whether the debouncer has a valid last_triggered time.
    /// This will be true if the debouncer is still waiting or if
    /// it is already complete, but has not been polled.
    pub fn active(&self) -> bool {
        self.last_triggered.is_some()
    }

    /// Returns a duration representing the time until poll will return true,
    /// or None if there is no more time to wait (even if poll has not yet been called).
    pub fn remaining(&self) -> Option<std::time::Duration> {
        if let Some(v) = self.last_triggered {
            let now = std::time::Instant::now();
            if now - v >= self.wait_time {
                None
            } else {
                Some(self.wait_time - (now - v))
            }
        } else {
            None
        }
    }
}
