// Copyright (c) 2018-present, Facebook, Inc.
// All Rights Reserved.
//
// This software may be used and distributed according to the terms of the
// GNU General Public License version 2 or any later version.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex, atomic::{AtomicUsize, Ordering}};

use cachelib::{get_cached, set_cached, Abomonation, LruCachePool};
use failure::prelude::*;

use CachelibKey;

#[derive(Clone)]
pub enum CachelibHandler<T> {
    Real(LruCachePool),
    #[allow(dead_code)] Mock(MockCachelib<T>),
}

impl<T> From<LruCachePool> for CachelibHandler<T> {
    fn from(cache: LruCachePool) -> Self {
        CachelibHandler::Real(cache)
    }
}

#[derive(Clone, Debug)]
pub struct MockCachelib<T> {
    cache: Arc<Mutex<HashMap<String, T>>>,
    get_count: Arc<AtomicUsize>,
}

impl<T> MockCachelib<T> {
    fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            get_count: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl<T: Abomonation + Clone + Send + 'static> CachelibHandler<T> {
    pub(crate) fn get_multiple_from_cachelib<Key: Eq + Hash>(
        &self,
        keys: Vec<(Key, CachelibKey)>,
    ) -> Result<(HashMap<Key, T>, Vec<(Key, CachelibKey)>)> {
        let mut fetched = HashMap::new();
        let mut left_to_fetch = Vec::new();

        for (key, cache_key) in keys {
            match self.get_cached(&cache_key.0)? {
                Some(value) => {
                    fetched.insert(key, value);
                }
                None => {
                    left_to_fetch.push((key, cache_key));
                }
            }
        }

        Ok((fetched, left_to_fetch))
    }

    pub(crate) fn fill_multiple_cachelib<Key: Eq + Hash>(
        &self,
        keys: HashMap<Key, (T, CachelibKey)>,
    ) -> HashMap<Key, T> {
        keys.into_iter()
            .map(|(key, (value, cache_key))| {
                // See comment in get_cached_or_fill why we ignore the result
                let _ = self.set_cached(&cache_key.0, &value);
                (key, value)
            })
            .collect()
    }

    fn get_cached(&self, key: &String) -> Result<Option<T>> {
        match self {
            CachelibHandler::Real(ref cache) => get_cached(cache, key),
            CachelibHandler::Mock(MockCachelib {
                ref cache,
                ref get_count,
                ..
            }) => {
                get_count.fetch_add(1, Ordering::SeqCst);
                Ok(cache.lock().expect("poisoned lock").get(key).cloned())
            }
        }
    }

    fn set_cached(&self, key: &String, value: &T) -> Result<bool> {
        match self {
            CachelibHandler::Real(ref cache) => set_cached(cache, key, value),
            CachelibHandler::Mock(MockCachelib { ref cache, .. }) => {
                cache
                    .lock()
                    .expect("poisoned lock")
                    .insert(key.clone(), value.clone());
                Ok(true)
            }
        }
    }

    #[allow(dead_code)]
    pub fn create_mock() -> Self {
        CachelibHandler::Mock(MockCachelib::new())
    }

    #[allow(dead_code)]
    pub(crate) fn gets_count(&self) -> usize {
        match self {
            CachelibHandler::Real(_) => unimplemented!(),
            CachelibHandler::Mock(MockCachelib { ref get_count, .. }) => {
                get_count.load(Ordering::SeqCst)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;

    use quickcheck::TestResult;

    quickcheck! {
        fn multiple_roundtrip(
            initial_keys: HashMap<String, String>,
            keys_to_query: HashSet<String>
        ) -> TestResult {
            let fill_query = initial_keys.clone().into_iter().map(|(key, val)| (key.clone(), (val, CachelibKey(key)))).collect();
            let get_query = keys_to_query.clone().into_iter().map(|key| (key.clone(),  CachelibKey(key))).collect();

            let mock_cachelib = MockCachelib::new();
            let cachelib_handler = CachelibHandler::Mock(mock_cachelib.clone());

            cachelib_handler.fill_multiple_cachelib(fill_query);

            if *mock_cachelib.cache.lock().expect("poisoned lock") != initial_keys {
                return TestResult::error("After fill_multiple_cachelib the content of cache is incorrect");
            }

            let (fetched, left) = cachelib_handler.get_multiple_from_cachelib(get_query).unwrap();

            for (key, cache_key) in &left {
                if key != &cache_key.0 {
                    return TestResult::error("Key and cache key got mixed in left!");
                }
                if initial_keys.get(key).is_some() {
                    return TestResult::error("After get_multiple_from_cachelib left is incorrect");
                }
            }

            for (key, val) in fetched.iter() {
                if initial_keys.get(key) != Some(val) {
                    return TestResult::error("Wrong value returned from get");
                }
            }

            if fetched.len() + left.len() != keys_to_query.len() {
                return TestResult::error("Returned wrong number of items from get");
            }

            let left: HashSet<_> = left.into_iter().map(|(key, _)| key).collect();
            let mut fetched: HashSet<_> = fetched.into_iter().map(|(key, _)| key).collect();

            if fetched.len() + left.len() != keys_to_query.len() {
                return TestResult::error("Returned wrong number of unique items from get");
            }

            fetched.extend(left);

            if fetched != keys_to_query {
                return TestResult::error("Didn't return all keys from get");
            }

            TestResult::passed()
        }
    }
}
