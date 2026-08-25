use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct EditHistory<T>
where
    T: PartialEq + Clone,
{
    history: Vec<T>,
    future: Vec<T>,
    current: T,
    last_change: Instant,
    wait: Duration,
}

impl<T> EditHistory<T>
where
    T: PartialEq + Clone,
{
    pub fn new(current: T, wait: std::time::Duration) -> Self {
        Self {
            history: vec![],
            future: vec![],
            current,
            last_change: Instant::now(),
            wait,
        }
    }

    pub fn undo(&mut self) -> Option<&T> {
        self.future
            .push(std::mem::replace(&mut self.current, self.history.pop()?));
        Some(&self.current)
    }

    pub fn redo(&mut self) -> Option<&T> {
        self.history
            .push(std::mem::replace(&mut self.current, self.future.pop()?));
        Some(&self.current)
    }

    pub fn update(&mut self, new: &T) {
        if &self.current == new {
            return;
        }
        let old = std::mem::replace(&mut self.current, T::clone(new));

        if Instant::now().duration_since(self.last_change) > self.wait {
            tracing::debug!(
                "Saving history: {:?}",
                Instant::now().duration_since(self.last_change)
            );
            self.history.push(old);
        }

        self.last_change = Instant::now();
        self.future.clear();
    }

    pub fn has_history(&self) -> bool {
        !self.history.is_empty()
    }

    pub fn has_future(&self) -> bool {
        !self.future.is_empty()
    }
}
