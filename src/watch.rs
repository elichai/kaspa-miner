use parking_lot::{Condvar, Mutex, RwLock};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

// The value is in a RWLock, all receivers observe it using a read lock + clone
// id is used to check if there's a new value before reading the old value.
// readers count is so that receivers and senders can know if everyone dropped their channels
// wait_for_change + notify_change is to allow receivers to wait for new values.

struct Shared<T: Clone> {
    value: RwLock<T>,
    id: AtomicUsize,
    wait_for_change: Mutex<()>,
    notify_change: Condvar,
    receivers_count: AtomicUsize,
    sender_alive: AtomicBool,
}

impl<T: Clone> Shared<T> {
    fn receiver_count(&self) -> usize {
        self.receivers_count.load(Ordering::Acquire)
    }

    fn increment_receiver_count(&self) {
        self.receivers_count.fetch_add(1, Ordering::Release);
    }

    fn decrement_receivers_count(&self) {
        self.receivers_count.fetch_sub(1, Ordering::Release);
    }

    fn drop_sender(&self) {
        self.sender_alive.store(false, Ordering::Release);
    }

    fn sender_alive(&self) -> bool {
        self.sender_alive.load(Ordering::Acquire)
    }

    fn replace_value(&self, val: T) {
        *self.value.write() = val;
    }

    fn increment_id(&self) -> usize {
        self.id.fetch_add(1, Ordering::Release)
    }

    fn id(&self) -> usize {
        self.id.load(Ordering::Acquire)
    }

    fn clone_value(&self) -> T {
        self.value.read().clone()
    }

    fn wake_up_threads(&self) {
        let _lock = self.wait_for_change.lock();
        self.notify_change.notify_all();
    }
}

pub fn channel<T: Clone>(init: T) -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Shared {
        value: RwLock::new(init),
        id: AtomicUsize::new(1),
        receivers_count: AtomicUsize::new(1),
        wait_for_change: Mutex::new(()),
        notify_change: Condvar::new(),
        sender_alive: AtomicBool::new(true),
    });
    let sender = Sender { shared: Arc::clone(&shared) };
    let receiver = Receiver { shared: Arc::clone(&shared), last_observed: 0 };
    (sender, receiver)
}

pub struct Sender<T: Clone> {
    shared: Arc<Shared<T>>,
}

impl<T: Clone> Sender<T> {
    pub fn send(&self, value: T) -> Result<(), ChannelClosed> {
        // if no receiver left, return an error
        if self.shared.receiver_count() == 0 {
            return Err(ChannelClosed(()));
        }
        self.shared.replace_value(value);
        // Signal that the value has been changed.
        self.shared.increment_id();
        // Notify in-case any receiver is waiting
        // Make sure no one is a moment before waiting
        self.shared.wake_up_threads();
        Ok(())
    }
}

impl<T: Clone> Drop for Sender<T> {
    fn drop(&mut self) {
        // Mark sender as dropped
        self.shared.drop_sender();
        // Make sure all waiting receivers will unlock and see that the sender was dropped.
        self.shared.wake_up_threads();
    }
}

pub struct Receiver<T: Clone> {
    shared: Arc<Shared<T>>,
    last_observed: usize,
}

impl<T: Clone> Receiver<T> {
    pub fn get_changed(&mut self) -> Result<Option<T>, ChannelClosed> {
        Self::get_changed_internal(&mut self.last_observed, &self.shared)
    }

    fn get_changed_internal(last_observed: &mut usize, shared: &Shared<T>) -> Result<Option<T>, ChannelClosed> {
        if !shared.sender_alive() {
            return Err(ChannelClosed(()));
        }
        let new_id = shared.id();
        if *last_observed == new_id {
            return Ok(None);
        }
        *last_observed = new_id;
        Ok(Some(shared.clone_value()))
    }

    pub fn wait_for_change(&mut self) -> Result<T, ChannelClosed> {
        if let Some(v) = self.get_changed()? {
            return Ok(v);
        }
        let lock = self.shared.wait_for_change.lock();
        // Check if while acquiring the lock something changed.
        if let Some(v) = Self::get_changed_internal(&mut self.last_observed, &self.shared)? {
            return Ok(v);
        }
        // wait for a notification of a new value
        let _ = self.shared.notify_change.wait(lock);
        // Recheck if the sender is alive as it might've changed while waiting
        if !self.shared.sender_alive() {
            return Err(ChannelClosed(()));
        }
        self.last_observed = self.shared.id();
        Ok(self.shared.clone_value())
    }
}

impl<T: Clone> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        self.shared.increment_receiver_count();
        Self { shared: Arc::clone(&self.shared), last_observed: self.last_observed }
    }
}

impl<T: Clone> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.shared.decrement_receivers_count();
    }
}

#[derive(Debug)]
pub struct ChannelClosed(());

impl Display for ChannelClosed {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("Channel is closed")
    }
}

impl Error for ChannelClosed {}
