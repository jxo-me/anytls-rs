//! Simple async DNS cache to reduce repeated lookups for popular domains.

use crate::util::{AnyTlsError, Result};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};
use tokio::net::lookup_host;
use tokio::sync::RwLock;
use tracing::{debug, trace};

/// TTL for cached DNS entries.
const DEFAULT_TTL: Duration = Duration::from_secs(60);
/// Timeout for DNS lookup operations.
const DNS_TIMEOUT: Duration = Duration::from_secs(10);

static DNS_CACHE: Lazy<DnsCache> = Lazy::new(DnsCache::new);

struct CacheEntry {
    addresses: Vec<SocketAddr>,
    expires_at: Instant,
    next_index: usize,
}

pub struct DnsCache {
    inner: RwLock<HashMap<String, CacheEntry>>,
}

impl DnsCache {
    fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    async fn get(&self, host: &str) -> Option<SocketAddr> {
        let cache = self.inner.read().await;
        if let Some(entry) = cache.get(host)
            && Instant::now() <= entry.expires_at
            && !entry.addresses.is_empty()
        {
            let index = entry.next_index % entry.addresses.len();
            let addr = entry.addresses[index];
            trace!("[DNS] Cache hit for {} -> {}", host, addr);
            return Some(addr);
        }
        None
    }

    async fn insert(&self, host: String, addresses: Vec<SocketAddr>) {
        let mut cache = self.inner.write().await;
        cache.insert(
            host,
            CacheEntry {
                addresses,
                expires_at: Instant::now() + DEFAULT_TTL,
                next_index: 0,
            },
        );
    }

    async fn advance(&self, host: &str) {
        let mut cache = self.inner.write().await;
        if let Some(entry) = cache.get_mut(host) {
            entry.next_index = entry.next_index.wrapping_add(1);
        }
    }
}

/// Resolve a hostname with caching and timeout.
pub async fn resolve_host_with_cache(host: &str, port: u16) -> Result<SocketAddr> {
    if let Some(addr) = DNS_CACHE.get(host).await {
        DNS_CACHE.advance(host).await;
        return Ok(addr);
    }

    let lookup_future = lookup_host((host, port));
    let mut addresses = tokio::time::timeout(DNS_TIMEOUT, lookup_future)
        .await
        .map_err(|_| {
            AnyTlsError::Protocol(format!(
                "DNS resolution timeout ({}s) for {}",
                DNS_TIMEOUT.as_secs(),
                host
            ))
        })?
        .map_err(|err| {
            AnyTlsError::Io(std::io::Error::other(format!(
                "DNS resolution failed for {}: {}",
                host, err
            )))
        })?
        .collect::<Vec<_>>();

    if addresses.is_empty() {
        return Err(AnyTlsError::Protocol(format!(
            "No address found for {}",
            host
        )));
    }

    // Sort to keep stability across runs (helps caching)
    addresses.sort_unstable_by_key(|addr| match addr.ip() {
        IpAddr::V4(ip) => (0, ip.octets().to_vec()),
        IpAddr::V6(ip) => (1, ip.octets().to_vec()),
    });

    debug!(
        "[DNS] Resolved {} -> {} entries (ttl={}s)",
        host,
        addresses.len(),
        DEFAULT_TTL.as_secs()
    );

    DNS_CACHE.insert(host.to_string(), addresses.clone()).await;
    DNS_CACHE.advance(host).await;
    Ok(addresses[0])
}
