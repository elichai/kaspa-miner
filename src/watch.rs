use std::error::Error;
use std::fmt::{Display, Formatter};

use sync::{Arc, AtomicBool, AtomicUsize, Condvar, Mutex, Ordering, RwLock};

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
        let mut lock = self.value.write();
        *lock = val;
        // Signal that the value has been changed, we do that while still holding the write lock
        // in order to make sure that a reader can't observe a new value with an old ID.
        self.increment_id();
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
        // Make sure no receiver is "almost" waiting (holding the lock but hasn't entered the Condvar yet)
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
    #[inline(always)]
    pub fn get_changed(&mut self) -> Result<Option<T>, ChannelClosed> {
        Self::get_changed_internal(&mut self.last_observed, &self.shared)
    }

    #[inline(always)]
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
        if let Some(v) = Self::get_changed_internal(&mut self.last_observed, &self.shared)? {
            return Ok(v);
        }
        let lock = self.shared.wait_for_change.lock();
        // Check if while acquiring the lock something changed.
        if let Some(v) = Self::get_changed_internal(&mut self.last_observed, &self.shared)? {
            return Ok(v);
        }
        // wait for a notification of a new value
        let _lock = self.shared.notify_change.wait(lock);
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

#[derive(Debug, PartialEq, Eq)]
pub struct ChannelClosed(());

impl Display for ChannelClosed {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("Channel is closed")
    }
}

impl Error for ChannelClosed {}

mod sync {
    #[cfg(all(feature = "parking_lot", feature = "shuttle"))]
    compile_error!("Can't use sync primitives both from parking_lot and from shuttle");

    #[cfg(feature = "parking_lot")]
    use parking::{
        Condvar as CondvarInternal, Mutex as MutexInternal, MutexGuard, RwLock as RwLockInternal, RwLockReadGuard,
        RwLockWriteGuard,
    };

    #[cfg(feature = "shuttle")]
    use shuttle::sync::{
        Condvar as CondvarInternal, Mutex as MutexInternal, MutexGuard, RwLock as RwLockInternal, RwLockReadGuard,
        RwLockWriteGuard,
    };
    #[cfg(not(any(feature = "shuttle", feature = "parking_lot")))]
    use std::sync::{
        Condvar as CondvarInternal, Mutex as MutexInternal, MutexGuard, RwLock as RwLockInternal, RwLockReadGuard,
        RwLockWriteGuard,
    };

    #[cfg(feature = "shuttle")]
    pub use shuttle::{
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Arc, Barrier,
        },
        thread,
    };
    #[cfg(not(feature = "shuttle"))]
    pub use std::{
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Arc, Barrier,
        },
        thread,
    };

    pub struct Mutex<T>(MutexInternal<T>);
    impl<T> Mutex<T> {
        pub fn new(val: T) -> Self {
            Self(MutexInternal::new(val))
        }

        #[inline(always)]
        pub fn lock(&self) -> MutexGuard<T> {
            #[cfg(not(feature = "parking_lot"))]
            return self.0.lock().unwrap_or_else(|e| e.into_inner());
            #[cfg(feature = "parking_lot")]
            return self.0.lock();
        }
    }

    pub struct Condvar(CondvarInternal);

    impl Condvar {
        pub fn new() -> Self {
            Self(CondvarInternal::new())
        }

        #[allow(unused_mut)]
        #[inline(always)]
        pub fn wait<'a, T>(&self, mut guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
            #[cfg(not(feature = "parking_lot"))]
            return self.0.wait(guard).unwrap_or_else(|e| e.into_inner());
            #[cfg(feature = "parking_lot")]
            {
                self.0.wait(&mut guard);
                guard
            }
        }

        #[inline(always)]
        pub fn notify_all(&self) {
            self.0.notify_all();
        }
    }

    pub struct RwLock<T>(RwLockInternal<T>);
    impl<T> RwLock<T> {
        pub fn new(val: T) -> Self {
            Self(RwLockInternal::new(val))
        }

        #[inline(always)]
        pub fn read(&self) -> RwLockReadGuard<T> {
            #[cfg(not(feature = "parking_lot"))]
            return self.0.read().unwrap_or_else(|e| e.into_inner());
            #[cfg(feature = "parking_lot")]
            return self.0.read();
        }

        #[inline(always)]
        pub fn write(&self) -> RwLockWriteGuard<T> {
            #[cfg(not(feature = "parking_lot"))]
            return self.0.write().unwrap_or_else(|e| e.into_inner());
            #[cfg(feature = "parking_lot")]
            return self.0.write();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::watch::{
        self,
        sync::{thread, Arc, Barrier},
        ChannelClosed,
    };

    #[test]
    fn test_receiver_count() {
        multi_test_runner(
            || {
                let (rx, tx) = watch::channel("One");
                assert_eq!(rx.shared.receiver_count(), 1);
                assert_eq!(tx.shared.receiver_count(), 1);
                let tx2 = tx.clone();
                assert_eq!(rx.shared.receiver_count(), 2);
                assert_eq!(tx.shared.receiver_count(), 2);
                assert_eq!(tx2.shared.receiver_count(), 2);
                let tx3 = tx.clone();
                assert_eq!(rx.shared.receiver_count(), 3);
                assert_eq!(tx.shared.receiver_count(), 3);
                assert_eq!(tx2.shared.receiver_count(), 3);
                assert_eq!(tx3.shared.receiver_count(), 3);
                drop(tx2);
                assert_eq!(rx.shared.receiver_count(), 2);
                assert_eq!(tx.shared.receiver_count(), 2);
                assert_eq!(tx3.shared.receiver_count(), 2);
                drop(tx3);
                drop(tx);
                assert_eq!(rx.shared.receiver_count(), 0);
                assert_eq!(rx.send("Two"), Err(ChannelClosed(())));
            },
            false,
        )
    }

    #[test]
    fn test_sender_dropped() {
        multi_test_runner(
            || {
                let (rx, mut tx) = watch::channel("One");
                assert_eq!(rx.shared.receiver_count(), 1);
                assert_eq!(tx.shared.receiver_count(), 1);
                assert!(rx.shared.sender_alive());
                assert!(tx.shared.sender_alive());
                drop(rx);
                assert_eq!(tx.shared.receiver_count(), 1);
                assert!(!tx.shared.sender_alive());
                assert_eq!(tx.get_changed(), Err(ChannelClosed(())));
            },
            false,
        )
    }

    #[test]
    fn test_sending_val() {
        multi_test_runner(
            || {
                let (rx, mut tx) = watch::channel("One");
                let mut tx2 = tx.clone();
                assert_eq!(tx.get_changed(), Ok(Some("One")));
                assert_eq!(tx.get_changed(), Ok(None));
                let mut tx3 = tx.clone();
                assert_eq!(tx3.get_changed(), Ok(None));
                assert_eq!(tx2.get_changed(), Ok(Some("One")));
                rx.send("Two").unwrap();
                assert_eq!(tx.get_changed(), Ok(Some("Two")));
                assert_eq!(tx2.get_changed(), Ok(Some("Two")));
                assert_eq!(tx3.get_changed(), Ok(Some("Two")));
            },
            false,
        )
    }

    #[test]
    fn test_sending_val_waiting() {
        multi_test_runner(
            || {
                let (rx, mut tx) = watch::channel("One");
                let mut tx2 = tx.clone();
                assert_eq!(tx.wait_for_change(), Ok("One"));
                assert_eq!(tx.get_changed(), Ok(None));
                let mut tx3 = tx.clone();
                assert_eq!(tx3.get_changed(), Ok(None));
                assert_eq!(tx2.wait_for_change(), Ok("One"));
                rx.send("Two").unwrap();
                assert_eq!(tx.wait_for_change(), Ok("Two"));
                assert_eq!(tx2.wait_for_change(), Ok("Two"));
                assert_eq!(tx3.wait_for_change(), Ok("Two"));
            },
            false,
        )
    }

    fn multi_test_runner(f: impl Fn() + Sync + Send + 'static, parallel: bool) {
        let mut iters = if parallel { 10_000 } else { 5 };
        if !cfg!(debug_assertions) {
            iters *= 10;
        }
        if cfg!(feature = "parking_lot") || cfg!(feature = "shuttle") {
            iters *= 10;
        }
        #[cfg(feature = "shuttle")]
        shuttle::check_random(f, iters);
        #[cfg(not(feature = "shuttle"))]
        for _ in 0..iters {
            f();
        }
    }

    #[test]
    fn test_waiting_on_val() {
        multi_test_runner(
            || {
                let (rx, mut tx) = watch::channel("One");
                assert_eq!(tx.get_changed(), Ok(Some("One")));
                let mut tx2 = tx.clone();
                assert_eq!(tx.get_changed(), Ok(None));
                assert_eq!(tx2.get_changed(), Ok(None));
                let barrier = Arc::new(Barrier::new(3));
                let barrier_clone = Arc::clone(&barrier);
                let handle1 = thread::spawn(move || {
                    barrier_clone.wait();
                    assert_eq!(tx.wait_for_change(), Ok("Two"));
                });
                let barrier_clone = Arc::clone(&barrier);
                let handle2 = thread::spawn(move || {
                    barrier_clone.wait();
                    assert_eq!(tx2.wait_for_change(), Ok("Two"));
                });
                barrier.wait();
                rx.send("Two").unwrap();

                handle1.join().unwrap();
                handle2.join().unwrap();
                assert_eq!(rx.shared.receiver_count(), 0);
            },
            true,
        )
    }

    #[test]
    fn test_rx_drop_before_waiting() {
        multi_test_runner(
            || {
                let (rx, mut tx) = watch::channel("One");
                assert_eq!(tx.get_changed(), Ok(Some("One")));
                assert_eq!(tx.get_changed(), Ok(None));
                drop(rx);
                assert_eq!(tx.wait_for_change(), Err(ChannelClosed(())));
            },
            false,
        )
    }

    #[test]
    fn test_rx_drop_while_waiting() {
        multi_test_runner(
            || {
                let (rx, mut tx) = watch::channel("One");
                assert_eq!(tx.get_changed(), Ok(Some("One")));
                assert_eq!(tx.get_changed(), Ok(None));
                let tx2 = tx.clone();
                assert_eq!(tx2.shared.receiver_count(), 2);
                let barrier = Arc::new(Barrier::new(3));
                let barrier_clone = Arc::clone(&barrier);
                let handle1 = thread::spawn(move || {
                    barrier_clone.wait();
                    assert_eq!(tx.wait_for_change(), Err(ChannelClosed(())));
                });
                let barrier_clone = Arc::clone(&barrier);
                let handle2 = thread::spawn(move || {
                    barrier_clone.wait();
                    drop(rx);
                });
                barrier.wait();
                handle1.join().unwrap();
                handle2.join().unwrap();
                assert!(!tx2.shared.sender_alive());
                assert_eq!(tx2.shared.receiver_count(), 1);
            },
            true,
        )
    }
}
