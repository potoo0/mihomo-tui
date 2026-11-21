use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt::{self, Debug, Formatter};
use std::sync::{Arc, RwLock};

use anyhow::{Result, anyhow, bail};

use crate::components::ComponentId;

/// The action a store chooses when its owning component is removed.
///
/// Stores retain their state by default. Stores that only hold runtime data for
/// specific components opt into [`StoreDisposition::RemoveIfNoOwners`] and are
/// removed from the hub once every owner is gone.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StoreDisposition {
    #[default]
    Retain,
    RemoveIfNoOwners(&'static [ComponentId]),
}

/// Per-store policy for owner-aware release.
pub trait StoreLifecycle: Send + Sync + 'static {
    fn disposition() -> StoreDisposition {
        StoreDisposition::Retain
    }

    /// Called when the store is removed from the hub because all of its
    /// declared owners are gone.
    fn on_release(&mut self) {}
}

/// A cloneable handle to one value in a [`StoreHub`].
///
/// Access is closure-based so locks are never exposed to callers and cannot be
/// held accidentally across an `.await`.
///
/// Closures run while the store lock is held. Do not call `read`, `update`,
/// or `snapshot` on the same store from within a closure, including through
/// cloned handles.
/// Debug builds detect same-thread reentry and panic before acquiring the lock.
pub struct StoreRef<T> {
    lock: Arc<RwLock<T>>,
}

impl<T> Clone for StoreRef<T> {
    fn clone(&self) -> Self {
        Self { lock: Arc::clone(&self.lock) }
    }
}

impl<T> Debug for StoreRef<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoreRef").finish_non_exhaustive()
    }
}

impl<T> StoreRef<T> {
    fn new(value: T) -> Self {
        Self { lock: Arc::new(RwLock::new(value)) }
    }

    #[cfg_attr(debug_assertions, track_caller)]
    pub fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        #[cfg(debug_assertions)]
        let _access = debug_access::StoreAccessGuard::enter(&self.lock);
        let guard = self.lock.read().expect("store lock poisoned");
        f(&guard)
    }

    #[cfg_attr(debug_assertions, track_caller)]
    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        #[cfg(debug_assertions)]
        let _access = debug_access::StoreAccessGuard::enter(&self.lock);
        let mut guard = self.lock.write().expect("store lock poisoned");
        f(&mut guard)
    }

    #[cfg_attr(debug_assertions, track_caller)]
    pub fn snapshot(&self) -> T
    where
        T: Clone,
    {
        self.read(T::clone)
    }
}

struct StoreCell<T>(StoreRef<T>);

trait StoreEntry: Send + Sync {
    fn as_any(&self) -> &dyn Any;

    fn type_name(&self) -> &'static str;

    fn disposition(&self) -> StoreDisposition;

    fn release(&self);
}

impl<T: StoreLifecycle> StoreEntry for StoreCell<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn type_name(&self) -> &'static str {
        std::any::type_name::<T>()
    }

    fn disposition(&self) -> StoreDisposition {
        T::disposition()
    }

    fn release(&self) {
        self.0.update(|store| store.on_release());
    }
}

type StoreEntryBox = Box<dyn StoreEntry>;

/// A dynamically mutable, type-keyed collection of shared state.
///
/// The key is the concrete value type's [`TypeId`], not a component id. Use
/// distinct newtypes when two independent stores happen to have the same shape.
#[derive(Clone, Default)]
pub struct StoreHub {
    entries: Arc<RwLock<HashMap<TypeId, StoreEntryBox>>>,
}

impl Debug for StoreHub {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut stores = self
            .entries
            .read()
            .expect("store hub lock poisoned")
            .values()
            .map(|entry| entry.type_name())
            .collect::<Vec<_>>();
        stores.sort_unstable();
        f.debug_struct("StoreHub").field("stores", &stores).finish()
    }
}

impl StoreHub {
    /// Register a new store, rejecting duplicate types.
    pub fn register<T>(&self, value: T) -> Result<StoreRef<T>>
    where
        T: StoreLifecycle,
    {
        let key = TypeId::of::<T>();
        let mut entries = self.entries.write().expect("store hub lock poisoned");
        if entries.contains_key(&key) {
            bail!("store `{}` is already registered", std::any::type_name::<T>());
        }

        let store = StoreRef::new(value);
        entries.insert(key, Box::new(StoreCell(store.clone())));
        Ok(store)
    }

    /// Register a store if it is missing, returning the existing handle otherwise.
    ///
    /// The initializer runs after a missing-store lookup; concurrent callers may
    /// each run it. On a registration race, the existing handle is returned.
    pub fn get_or_register<T>(&self, init: impl FnOnce() -> Result<T>) -> Result<StoreRef<T>>
    where
        T: StoreLifecycle,
    {
        if let Some(store) = self.try_get::<T>() {
            return Ok(store);
        }

        let value = init()?;
        match self.register(value) {
            Ok(store) => Ok(store),
            // Another caller won the initialization race.
            Err(_) => self.get::<T>(),
        }
    }

    /// Get a typed handle or report that the store is missing.
    pub fn get<T>(&self) -> Result<StoreRef<T>>
    where
        T: Send + Sync + 'static,
    {
        let entries = self.entries.read().expect("store hub lock poisoned");
        entries
            .get(&TypeId::of::<T>())
            .and_then(|entry| entry.as_any().downcast_ref::<StoreCell<T>>())
            .map(|cell| cell.0.clone())
            .ok_or_else(|| anyhow!("store `{}` is not registered", std::any::type_name::<T>()))
    }

    pub fn try_get<T>(&self) -> Option<StoreRef<T>>
    where
        T: Send + Sync + 'static,
    {
        self.get::<T>().ok()
    }

    /// Remove a store from the hub.
    ///
    /// Existing handles keep the old state alive. A later `get` for the same
    /// type fails until it is registered again.
    #[allow(dead_code)]
    pub fn unregister<T>(&self) -> Result<()>
    where
        T: Send + Sync + 'static,
    {
        let removed =
            self.entries.write().expect("store hub lock poisoned").remove(&TypeId::of::<T>());
        if removed.is_none() {
            bail!("store `{}` is not registered", std::any::type_name::<T>());
        }
        Ok(())
    }

    /// Remove stores whose declared owners are all gone.
    pub fn release_unused(&self, is_alive: impl Fn(ComponentId) -> bool) {
        let removed = {
            let mut entries = self.entries.write().expect("store hub lock poisoned");
            let stale = entries
                .iter()
                .filter_map(|(key, entry)| match entry.disposition() {
                    StoreDisposition::Retain => None,
                    StoreDisposition::RemoveIfNoOwners(owners) => {
                        owners.iter().all(|owner| !is_alive(*owner)).then_some(*key)
                    }
                })
                .collect::<Vec<_>>();
            stale.iter().filter_map(|key| entries.remove(key)).collect::<Vec<_>>()
        };

        for entry in removed {
            entry.release();
        }
    }
}

#[cfg(debug_assertions)]
mod debug_access {
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::sync::{Arc, RwLock};

    thread_local! {
        static ACTIVE_STORES: RefCell<HashSet<*const ()>> = RefCell::default();
    }

    pub(super) struct StoreAccessGuard(*const ());

    impl StoreAccessGuard {
        #[track_caller]
        pub fn enter<T>(lock: &Arc<RwLock<T>>) -> Self {
            let id = Arc::as_ptr(lock).cast::<()>();
            let entered = ACTIVE_STORES.with(|stores| stores.borrow_mut().insert(id));
            assert!(entered, "reentrant access to store `{}`", std::any::type_name::<T>());
            Self(id)
        }
    }

    impl Drop for StoreAccessGuard {
        fn drop(&mut self) {
            ACTIVE_STORES.with(|stores| stores.borrow_mut().remove(&self.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::components::ComponentId;

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct Counter(i32);

    impl StoreLifecycle for Counter {}

    #[test]
    fn debug_lists_registered_types_without_locking_store_values() {
        struct NoDebug;
        impl StoreLifecycle for NoDebug {}

        let hub = StoreHub::default();
        let store = hub.register(NoDebug).unwrap();
        hub.register(Counter(1)).unwrap();
        store.update(|_| {
            let debug = format!("{hub:?}");
            assert!(debug.contains(std::any::type_name::<NoDebug>()));
            assert!(debug.contains(std::any::type_name::<Counter>()));
        });
        hub.unregister::<NoDebug>().unwrap();
        assert!(!format!("{hub:?}").contains(std::any::type_name::<NoDebug>()));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn cloned_handles_detect_reentry_for_every_access_method() {
        let accesses: [fn(&StoreRef<Counter>); 3] = [
            |store| store.read(|_| ()),
            |store| store.update(|_| ()),
            |store| {
                let _ = store.snapshot();
            },
        ];
        for write_outer in [false, true] {
            for inner in accesses {
                let store = StoreRef::new(Counter(1));
                let cloned = store.clone();
                let nested_access = |_: &Counter| inner(&cloned);
                let panic = std::panic::catch_unwind(|| {
                    if write_outer {
                        store.update(|counter| nested_access(counter));
                    } else {
                        store.read(nested_access);
                    }
                })
                .unwrap_err();
                let message = panic.downcast_ref::<String>().unwrap();
                assert!(message.contains("reentrant access to store"));
                assert!(message.contains(std::any::type_name::<Counter>()));
            }
        }
    }

    #[cfg(debug_assertions)]
    #[test]
    fn access_tracking_is_cleared_after_return_and_unwind() {
        let store = StoreRef::new(Counter(1));
        assert_eq!(store.snapshot(), Counter(1));
        let panic = std::panic::catch_unwind(|| store.read(|_| panic!("reader failed")));
        assert!(panic.is_err());
        store.update(|counter| counter.0 += 1);
        assert_eq!(store.snapshot(), Counter(2));
    }

    #[test]
    fn different_stores_can_be_accessed_in_nested_closures() {
        let left = StoreRef::new(Counter(1));
        let right = StoreRef::new(Counter(2));
        left.read(|left| right.update(|right| right.0 += left.0));
        assert_eq!(right.snapshot(), Counter(3));
    }

    #[test]
    fn shared_reads_from_different_threads_are_allowed() {
        let store = StoreRef::new(Counter(1));
        store.read(|_| {
            std::thread::scope(|scope| {
                scope.spawn(|| assert_eq!(store.snapshot(), Counter(1))).join().unwrap();
            });
        });
    }

    #[test]
    fn register_rejects_duplicate_types() {
        let hub = StoreHub::default();
        hub.register(Counter(1)).unwrap();
        let error = hub.register(Counter(2)).unwrap_err();
        assert!(error.to_string().contains("already registered"));
    }

    #[test]
    fn get_or_register_initializes_only_on_first_use() {
        let hub = StoreHub::default();
        let calls = AtomicUsize::new(0);

        let first = hub
            .get_or_register(|| {
                calls.fetch_add(1, Ordering::Relaxed);
                Ok(Counter(1))
            })
            .unwrap();
        let second = hub
            .get_or_register(|| {
                calls.fetch_add(1, Ordering::Relaxed);
                Ok(Counter(2))
            })
            .unwrap();

        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(first.snapshot(), second.snapshot());
    }

    #[test]
    fn get_reports_missing_store() {
        let hub = StoreHub::default();
        let error = hub.get::<Counter>().unwrap_err();
        assert!(error.to_string().contains("is not registered"));
    }

    #[test]
    fn cloned_refs_share_state() {
        let hub = StoreHub::default();
        let left = hub.register(Counter(1)).unwrap();
        let right = left.clone();
        left.update(|counter| counter.0 += 1);
        assert_eq!(right.read(|counter| *counter), Counter(2));
    }

    #[test]
    fn hubs_are_isolated() {
        let left = StoreHub::default();
        let right = StoreHub::default();
        left.register(Counter(1)).unwrap().update(|counter| counter.0 += 1);
        right.register(Counter(10)).unwrap();

        assert_eq!(left.get::<Counter>().unwrap().snapshot(), Counter(2));
        assert_eq!(right.get::<Counter>().unwrap().snapshot(), Counter(10));
    }

    #[test]
    fn unregister_keeps_existing_refs_alive() {
        let hub = StoreHub::default();
        let store = hub.register(Counter(1)).unwrap();
        hub.unregister::<Counter>().unwrap();

        assert!(hub.get::<Counter>().is_err());
        store.update(|counter| counter.0 += 1);
        assert_eq!(store.snapshot(), Counter(2));
    }

    #[test]
    fn release_unused_does_not_lock_retained_store() {
        let hub = StoreHub::default();
        let store = hub.register(Counter(1)).unwrap();

        store.update(|counter| {
            hub.release_unused(|_| false);
            counter.0 += 1;
        });

        assert_eq!(hub.get::<Counter>().unwrap().snapshot(), Counter(2));
    }

    #[test]
    fn release_unused_removes_only_stores_without_owners() {
        #[derive(Default)]
        struct Removable;

        impl StoreLifecycle for Removable {
            fn disposition() -> StoreDisposition {
                StoreDisposition::RemoveIfNoOwners(&[ComponentId::Root, ComponentId::Header])
            }
        }

        let hub = StoreHub::default();
        hub.register(Counter(1)).unwrap();
        hub.register(Removable).unwrap();

        hub.release_unused(|owner| owner == ComponentId::Header);
        assert!(hub.get::<Counter>().is_ok());
        assert!(hub.get::<Removable>().is_ok());

        hub.release_unused(|_| false);
        assert!(hub.get::<Counter>().is_ok());
        assert!(hub.get::<Removable>().is_err());
    }
}
