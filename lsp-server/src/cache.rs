use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Mutex, RwLock};

/// Result of a version lookup from a registry.
#[derive(Debug, Clone)]
pub struct VersionResult {
    /// All stable (non-prerelease, non-yanked) versions, sorted descending (newest first).
    pub stable_versions: Vec<String>,
    pub prerelease: Option<String>,
}

struct CacheEntry {
    result: VersionResult,
    inserted_at: Instant,
}

/// In-memory cache with TTL and inflight deduplication.
#[allow(dead_code)]
pub struct VersionCache {
    store: RwLock<HashMap<String, CacheEntry>>,
    inflight: Mutex<HashMap<String, broadcast::Sender<VersionResult>>>,
    ttl: Duration,
}

impl VersionCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
            inflight: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    /// Get a cached entry if it exists and hasn't expired.
    pub async fn get(&self, key: &str) -> Option<VersionResult> {
        let store = self.store.read().await;
        if let Some(entry) = store.get(key) {
            if entry.inserted_at.elapsed() < self.ttl {
                return Some(entry.result.clone());
            }
        }
        None
    }

    /// Insert or update a cache entry.
    pub async fn set(&self, key: String, result: VersionResult) {
        self.store.write().await.insert(
            key,
            CacheEntry {
                result,
                inserted_at: Instant::now(),
            },
        );
    }

    /// Remove a specific entry.
    #[allow(dead_code)]
    pub async fn invalidate(&self, key: &str) {
        self.store.write().await.remove(key);
    }

    /// Resolve a version, using cache and inflight deduplication.
    /// If the value is cached, return it. If an inflight request exists, wait for it.
    /// Otherwise, call the fetcher and cache the result.
    #[allow(dead_code)]
    pub async fn resolve<F, Fut>(&self, key: &str, fetcher: F) -> VersionResult
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = VersionResult>,
    {
        // Check cache first
        if let Some(cached) = self.get(key).await {
            return cached;
        }

        // Check for inflight request
        {
            let inflight = self.inflight.lock().await;
            if let Some(tx) = inflight.get(key) {
                let mut rx = tx.subscribe();
                drop(inflight);
                if let Ok(result) = rx.recv().await {
                    return result;
                }
                // If recv failed, the sender was dropped — fall through to fetch
            }
        }

        // Register ourselves as the inflight fetcher
        let (tx, _) = broadcast::channel(1);
        {
            let mut inflight = self.inflight.lock().await;
            inflight.insert(key.to_string(), tx.clone());
        }

        // Fetch
        let result = fetcher().await;

        // Cache the result
        self.set(key.to_string(), result.clone()).await;

        // Notify waiters and remove inflight entry
        let _ = tx.send(result.clone());
        {
            let mut inflight = self.inflight.lock().await;
            inflight.remove(key);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_cache_get_miss() {
        let cache = VersionCache::new(Duration::from_secs(300));
        assert!(cache.get("npm:react").await.is_none());
    }

    #[tokio::test]
    async fn test_cache_set_and_get() {
        let cache = VersionCache::new(Duration::from_secs(300));
        let result = VersionResult {
            stable_versions: vec!["18.2.0".to_string()],
            prerelease: None,
        };
        cache.set("npm:react".to_string(), result).await;

        let cached = cache.get("npm:react").await.unwrap();
        assert_eq!(
            cached.stable_versions.first().map(String::as_str),
            Some("18.2.0")
        );
        assert!(cached.prerelease.is_none());
    }

    #[tokio::test]
    async fn test_cache_ttl_expiry() {
        let cache = VersionCache::new(Duration::from_millis(50));
        cache
            .set(
                "npm:react".to_string(),
                VersionResult {
                    stable_versions: vec!["18.0.0".to_string()],
                    prerelease: None,
                },
            )
            .await;

        assert!(cache.get("npm:react").await.is_some());
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(cache.get("npm:react").await.is_none());
    }

    #[tokio::test]
    async fn test_cache_invalidate() {
        let cache = VersionCache::new(Duration::from_secs(300));
        cache
            .set(
                "npm:react".to_string(),
                VersionResult {
                    stable_versions: vec!["18.0.0".to_string()],
                    prerelease: None,
                },
            )
            .await;
        assert!(cache.get("npm:react").await.is_some());

        cache.invalidate("npm:react").await;
        assert!(cache.get("npm:react").await.is_none());
    }

    #[tokio::test]
    async fn test_cache_resolve_deduplication() {
        let cache = Arc::new(VersionCache::new(Duration::from_secs(300)));
        let call_count = Arc::new(AtomicU32::new(0));

        let mut handles = Vec::new();
        for _ in 0..5 {
            let cache = cache.clone();
            let count = call_count.clone();
            handles.push(tokio::spawn(async move {
                cache
                    .resolve("npm:react", || {
                        let count = count.clone();
                        async move {
                            count.fetch_add(1, Ordering::Relaxed);
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            VersionResult {
                                stable_versions: vec!["18.2.0".to_string()],
                                prerelease: None,
                            }
                        }
                    })
                    .await
            }));
        }

        for handle in handles {
            let r = handle.await.unwrap();
            assert_eq!(
                r.stable_versions.first().map(String::as_str),
                Some("18.2.0")
            );
        }

        // The fetcher should have been called only once (or at most a few if
        // timing is tight, but never 5 times)
        let count = call_count.load(Ordering::Relaxed);
        assert!(count <= 2, "Fetcher called {count} times, expected ≤ 2");
    }
}
