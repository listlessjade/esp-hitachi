use std::{cell::RefCell, sync::Arc, thread::LocalKey};

use arc_swap::{cache::Cache as ArcCache, ArcSwap, Guard};
use serde::{de::DeserializeOwned, Serialize};

#[macro_export]
macro_rules! impl_conf_type {
    ($for:path, $path:expr, $store:ident) => {
        use arc_swap::{ArcSwap, cache::Cache as ArcCache};
        use std::cell::RefCell;
        use std::sync::Arc;
        use std::ops::Deref;

        static $store: std::sync::LazyLock<arc_swap::ArcSwap<$for>> =
            std::sync::LazyLock::new(|| arc_swap::ArcSwap::from_pointee(<$for as ConfigType>::load_from_file().unwrap().unwrap_or_default()));


        impl ConfigType for $for {
            const PATH: &str = $path;
            thread_local!(static CACHE: RefCell<ArcCache<&'static ArcSwap<$for>, Arc<$for>>> = RefCell::new(ArcCache::from($store.deref())));

            fn store(self) -> anyhow::Result<arc_swap::Guard<std::sync::Arc<Self>>> {
                let f = std::fs::File::create(Self::PATH)?;
                serde_json::to_writer(f, &self)?;
                $store.store(self.into());

                Ok($store.load())
            }

            fn load_from_file() -> anyhow::Result<Option<Self>> {
                if !std::fs::exists(Self::PATH)? {
                    return Ok(None);
                }

                let f = std::fs::File::open(Self::PATH)?;
                Ok(Some(serde_json::from_reader(f)?))
            }

            fn read() -> arc_swap::Guard<std::sync::Arc<Self>> {
                $store.load()
            }

            // fn read_cached() -> ar
        }
    };
}

pub trait ConfigType: Serialize + DeserializeOwned + 'static {
    const PATH: &str;
    const CACHE: LocalKey<RefCell<ArcCache<&'static ArcSwap<Self>, Arc<Self>>>>;

    fn store(self) -> anyhow::Result<Guard<Arc<Self>>>;
    //  {
    // let f = File::create(Self::PATH)?;
    // serde_json::to_writer(f, self)?;
    // Ok(())
    // }

    fn read() -> Guard<Arc<Self>>;

    fn load_from_file() -> anyhow::Result<Option<Self>>;

    // }

    // fn default() -> Option<Self> { None }
}
