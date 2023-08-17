use arc_swap::{ArcSwapOption, Guard, RefCnt};
use std::ops::Deref;
use sync::{Arc, Condvar, Mutex};

pub struct Shared<T> {
    inner: ArcSwapOption<T>,
    wait_cv: Condvar,
    wait_mutex: Mutex<()>,
}

pub struct WatchSwap<T> {
    shared: Arc<Shared<T>>,
    cached: Option<Arc<T>>,
}

impl<T> Clone for WatchSwap<T> {
    fn clone(&self) -> Self {
        Self { shared: Arc::clone(&self.shared), cached: self.cached.clone() }
    }
}

impl<T> WatchSwap<T> {
    #[allow(dead_code)]
    pub fn init(val: impl Into<Option<T>>) -> Self {
        let val = val.into().map(Arc::new);
        Self {
            shared: Arc::new(Shared {
                inner: ArcSwapOption::new(val.clone()),
                wait_cv: Condvar::new(),
                wait_mutex: Mutex::new(()),
            }),
            cached: val,
        }
    }
    pub fn empty() -> Self {
        Self {
            shared: Arc::new(Shared {
                inner: ArcSwapOption::const_empty(),
                wait_cv: Condvar::new(),
                wait_mutex: Mutex::new(()),
            }),
            cached: None,
        }
    }

    fn wake_up_threads(&self) {
        // Make sure no receiver is "almost" waiting (holding the lock but hasn't entered the Condvar yet)
        let _lock = self.shared.wait_mutex.lock();
        self.shared.wait_cv.notify_all();
    }

    #[inline]
    fn get_changed_inner<'a>(cached: &'a mut Option<Arc<T>>, inner: &'a ArcSwapOption<T>) -> bool {
        // TODO: Optimize using `arc_swap::Cache` when https://github.com/vorner/arc-swap/pull/91 is merged.
        let cur_ptr = RefCnt::as_ptr(&*cached);
        let cheap_load = inner.load();
        if cur_ptr != RefCnt::as_ptr(&*cheap_load) {
            *cached = Guard::into_inner(cheap_load);
            true
        } else {
            false
        }
    }

    #[inline]
    pub fn get_changed(&mut self) -> Option<impl Deref<Target = Option<Arc<T>>> + '_> {
        Self::get_changed_inner(&mut self.cached, &self.shared.inner).then_some(&self.cached)
    }

    #[allow(dead_code)]
    pub fn peek_cached(&self) -> impl Deref<Target = Option<Arc<T>>> + '_ {
        &self.cached
    }

    pub fn swap(&self, val: impl Into<Option<T>>) -> Option<Arc<T>> {
        let old = self.shared.inner.swap(val.into().map(Arc::new));
        self.wake_up_threads();
        old
    }

    pub fn wait_for_change(&mut self) -> impl Deref<Target = Option<Arc<T>>> + '_ {
        let mut guard = self.shared.wait_mutex.lock();
        loop {
            if Self::get_changed_inner(&mut self.cached, &self.shared.inner) {
                return &self.cached;
            }
            guard = self.shared.wait_cv.wait(guard);
        }
    }
}

mod sync {
    #[cfg(all(feature = "parking_lot", feature = "shuttle"))]
    compile_error!("Can't use sync primitives both from parking_lot and from shuttle");

    #[cfg(feature = "parking_lot")]
    use parking::{Condvar as CondvarInternal, Mutex as MutexInternal, MutexGuard};

    #[cfg(feature = "shuttle")]
    use shuttle::sync::{Condvar as CondvarInternal, Mutex as MutexInternal, MutexGuard};
    #[cfg(not(any(feature = "shuttle", feature = "parking_lot")))]
    use std::sync::{Condvar as CondvarInternal, Mutex as MutexInternal, MutexGuard};

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
        sync::{Arc, Barrier},
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
}

#[cfg(test)]
mod tests {
    use crate::swap_rust::{
        sync::{thread, Arc, Barrier},
        WatchSwap,
    };

    fn channel<T>(val: T) -> (WatchSwap<T>, WatchSwap<T>) {
        let a = WatchSwap::empty();
        let b = a.clone();
        b.clone().swap(val);
        (a, b)
    }

    #[test]
    fn test_sending_val() {
        multi_test_runner(
            || {
                let (rx, mut tx) = channel("One");
                let mut tx2 = tx.clone();
                assert_eq!(tx.get_changed().as_deref().map(|a| a.as_deref()), Some(Some(&"One")));
                assert_eq!(tx.get_changed().as_deref().map(|a| a.as_deref()), None);
                let mut tx3 = tx.clone();
                assert_eq!(tx3.get_changed().as_deref().map(|a| a.as_deref()), None);
                assert_eq!(tx2.get_changed().as_deref().map(|a| a.as_deref()), Some(Some(&"One")));
                rx.swap("Two");
                assert_eq!(tx.get_changed().as_deref().map(|a| a.as_deref()), Some(Some(&"Two")));
                assert_eq!(tx2.get_changed().as_deref().map(|a| a.as_deref()), Some(Some(&"Two")));
                assert_eq!(tx3.get_changed().as_deref().map(|a| a.as_deref()), Some(Some(&"Two")));
            },
            false,
        )
    }

    #[test]
    fn test_sending_val_waiting() {
        multi_test_runner(
            || {
                let (rx, mut tx) = channel("One");
                let mut tx2 = tx.clone();
                assert_eq!(tx.wait_for_change().as_deref().copied(), Some("One"));
                assert_eq!(tx.get_changed().as_deref().map(|a| a.as_deref()), None);
                let mut tx3 = tx.clone();
                assert_eq!(tx3.get_changed().as_deref().map(|a| a.as_deref()), None);
                assert_eq!(tx2.wait_for_change().as_deref().copied(), Some("One"));
                rx.swap("Two").unwrap();
                assert_eq!(tx.wait_for_change().as_deref().copied(), Some("Two"));
                assert_eq!(tx2.wait_for_change().as_deref().copied(), Some("Two"));
                assert_eq!(tx3.wait_for_change().as_deref().copied(), Some("Two"));
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
                let (rx, mut tx) = channel("One");
                assert_eq!(tx.get_changed().as_deref().map(|a| a.as_deref()), Some(Some(&"One")));
                let mut tx2 = tx.clone();
                assert_eq!(tx.get_changed().as_deref().map(|a| a.as_deref()), None);
                assert_eq!(tx2.get_changed().as_deref().map(|a| a.as_deref()), None);
                let barrier = Arc::new(Barrier::new(3));
                let barrier_clone = Arc::clone(&barrier);
                let handle1 = thread::spawn(move || {
                    barrier_clone.wait();
                    assert_eq!(tx.wait_for_change().as_deref().copied(), Some("Two"));
                });
                let barrier_clone = Arc::clone(&barrier);
                let handle2 = thread::spawn(move || {
                    barrier_clone.wait();
                    assert_eq!(tx2.wait_for_change().as_deref().copied(), Some("Two"));
                });
                barrier.wait();
                rx.swap("Two");

                handle1.join().unwrap();
                handle2.join().unwrap();
            },
            true,
        )
    }
}
