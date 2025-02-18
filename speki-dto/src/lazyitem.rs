use std::{
    ops::{Deref, DerefMut}, sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard}, 
};
use crate::Item;
use crate::SpekiProvider;


/// Enforces variants such as cache whenever the item changes.
/// if an invariant enforcer function is provided, then on each write call, the old item gets cloned,
/// and when the write guard is dropped we pass in the old and new version of the item to the enforcer
/// so that it may update any necessary caches.
struct InvariantEnforcer<A: Item>{
    old_item: A,
    f: Arc<Box<dyn Fn(&A, &A)>>,
}

#[derive(Clone)]
pub struct LazyItem<A: Item> {
    key: A::Key,
    provider: Arc<Box<dyn SpekiProvider<A>>>,
    item: Arc<async_once_cell::OnceCell<Option<Arc<RwLock<A>>>>>,
    f: Option<Arc<Box<dyn Fn(&A, &A)>>>,
}

pub struct LazyWriteGuard<'a, A: Item>{
    guard: RwLockWriteGuard<'a, A>,
    provider: Arc<Box<dyn SpekiProvider<A>>>,
    enforcer: Option<InvariantEnforcer<A>>,
}

impl<A: Item> Drop for LazyWriteGuard<'_, A> {
    fn drop(&mut self) {
        let provider = self.provider.clone();
        let item: A = self.guard.clone();
        if let Some(enforcer) = &self.enforcer {
            let new_item = item.clone();
            (enforcer.f)(&enforcer.old_item, &new_item);
        }
        wasm_bindgen_futures::spawn_local(async move{
            provider.save_item(item).await;
        });
    }
}

impl<A: Item> Deref for LazyWriteGuard<'_, A> {
    type Target = A;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}


impl<A: Item> DerefMut for LazyWriteGuard<'_, A> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

impl<A: Item> LazyItem<A> {
    pub fn new(key: A::Key, provider: Arc<Box<dyn SpekiProvider<A>>>) -> Self {
        Self {
            key,
            provider,
            item: Default::default(),
            f: None,
        }
    }

    pub fn with_enforcer(mut self, f: Arc<Box<dyn Fn(&A, &A)>>) -> Self {
        self.f = Some(f);
        self
    }

    pub async fn new_with_value(item: A, provider: Arc<Box<dyn SpekiProvider<A>>>) -> Self {
        let key = item.id();
        provider.save_item(item).await;
        Self::new(key, provider)
    }


    async fn try_get(&self) -> Option<&Arc<RwLock<A>>> {
        self.item.get_or_init(async {
            match self.provider.load_item(self.key).await {
                Some(item) => Some(Arc::new(RwLock::new(item))),
                None => None,
            }
        }).await.as_ref()
    }


    pub async fn try_write(&self) -> Option<LazyWriteGuard<A>> {
        let guard = self.try_get().await?.write().unwrap();
        let enforcer: Option<InvariantEnforcer<A>> = match self.f.clone() { 
            Some(f) => {
                Some(
                    InvariantEnforcer {
                        old_item: guard.clone(),
                        f,
                    }
                )
            },
            None => None,
        };

        Some(LazyWriteGuard {
            guard,
            provider: self.provider.clone(),
            enforcer,
        })
    }

    pub async fn try_read(&self) -> Option<RwLockReadGuard<A>> {
        Some(self.try_get().await?.read().unwrap())
    }

    pub async fn write(&self) -> LazyWriteGuard<A> {
        self.try_write().await.unwrap()
    }


    pub async fn write_or_init<F: FnOnce() -> A>(&self, f: F) -> LazyWriteGuard<A> {
        if let Some(guard) = self.try_write().await {
            return guard;
        }

        self.item.get_or_init(async {Some(Arc::new(RwLock::new(f())))}).await;
        self.write().await
    }


    pub async fn read_or_init<F: FnOnce() -> A>(&self, f: F) -> RwLockReadGuard<A> {
        if let Some(guard) = self.try_read().await {
            return guard;
        }

        self.item.get_or_init(async {Some(Arc::new(RwLock::new(f())))}).await;
        self.read().await
    }


    pub async fn read(&self) -> RwLockReadGuard<A> {
        self.try_read().await.unwrap()
    }
}