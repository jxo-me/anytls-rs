//! Simple async DNS cache to reduce repeated lookups for popular domains.

use crate::util::{AnyTlsError, Result};
use hickory_resolver::TokioResolver;
use hickory_resolver::config::{ConnectionConfig, NameServerConfig, ResolverConfig};
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::lookup_host;
use tokio::sync::RwLock;
use tracing::{debug, info, trace};

/// TTL for cached DNS entries.
const DEFAULT_TTL: Duration = Duration::from_secs(60);
/// Timeout for DNS lookup operations.
const DNS_TIMEOUT: Duration = Duration::from_secs(10);
/// Bound memory used by hostnames supplied by remote clients.
const MAX_CACHE_ENTRIES: usize = 4096;

static DNS_CACHE: Lazy<DnsCache> = Lazy::new(DnsCache::new);
static DNS_RESOLVER: Lazy<RwLock<Option<Arc<TokioResolver>>>> = Lazy::new(|| RwLock::new(None));

struct CacheEntry {
    addresses: Vec<IpAddr>,
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

    async fn get(&self, host: &str, port: u16) -> Option<Vec<SocketAddr>> {
        let mut cache = self.inner.write().await;
        let now = Instant::now();
        if cache
            .get(host)
            .is_some_and(|entry| now > entry.expires_at || entry.addresses.is_empty())
        {
            cache.remove(host);
            return None;
        }

        let entry = cache.get_mut(host)?;
        let index = entry.next_index % entry.addresses.len();
        entry.next_index = (index + 1) % entry.addresses.len();
        let addresses = entry.addresses[index..]
            .iter()
            .chain(&entry.addresses[..index])
            .map(|ip| SocketAddr::new(*ip, port))
            .collect::<Vec<_>>();
        trace!(
            "[DNS] Cache hit for {} -> {} addresses",
            host,
            addresses.len()
        );
        Some(addresses)
    }

    async fn insert(&self, host: String, addresses: Vec<IpAddr>) {
        let mut cache = self.inner.write().await;
        if cache.len() >= MAX_CACHE_ENTRIES && !cache.contains_key(&host) {
            let now = Instant::now();
            cache.retain(|_, entry| entry.expires_at > now);
            if cache.len() >= MAX_CACHE_ENTRIES
                && let Some(oldest) = cache
                    .iter()
                    .min_by_key(|(_, entry)| entry.expires_at)
                    .map(|(host, _)| host.clone())
            {
                cache.remove(&oldest);
            }
        }
        cache.insert(
            host,
            CacheEntry {
                addresses,
                expires_at: Instant::now() + DEFAULT_TTL,
                next_index: 1,
            },
        );
    }

    async fn clear(&self) {
        let mut cache = self.inner.write().await;
        cache.clear();
    }
}

/// Resolve a hostname with caching and timeout.
pub async fn resolve_host_with_cache(host: &str, port: u16) -> Result<SocketAddr> {
    resolve_host_addresses_with_cache(host, port)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| AnyTlsError::Protocol(format!("No address found for {}", host)))
}

/// Resolve all candidate addresses, preserving the requested port and rotating
/// the starting address on cache hits.
pub async fn resolve_host_addresses_with_cache(host: &str, port: u16) -> Result<Vec<SocketAddr>> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(vec![SocketAddr::new(ip, port)]);
    }

    if let Some(addresses) = DNS_CACHE.get(host, port).await {
        return Ok(addresses);
    }

    let resolver_opt = DNS_RESOLVER.read().await.clone();
    let mut addresses: Vec<IpAddr> = if let Some(resolver) = resolver_opt {
        let lookup = tokio::time::timeout(DNS_TIMEOUT, resolver.lookup_ip(host))
            .await
            .map_err(|_| {
                AnyTlsError::Protocol(format!(
                    "DNS resolution timeout ({}s) for {}",
                    DNS_TIMEOUT.as_secs(),
                    host
                ))
            })?
            .map_err(|err| {
                AnyTlsError::Io(Error::other(format!(
                    "DNS resolution failed for {}: {}",
                    host, err
                )))
            })?;
        lookup.iter().collect()
    } else {
        let lookup_future = lookup_host((host, 0));
        tokio::time::timeout(DNS_TIMEOUT, lookup_future)
            .await
            .map_err(|_| {
                AnyTlsError::Protocol(format!(
                    "DNS resolution timeout ({}s) for {}",
                    DNS_TIMEOUT.as_secs(),
                    host
                ))
            })?
            .map_err(|err| {
                AnyTlsError::Io(Error::other(format!(
                    "DNS resolution failed for {}: {}",
                    host, err
                )))
            })?
            .map(|addr| addr.ip())
            .collect::<Vec<_>>()
    };

    if addresses.is_empty() {
        return Err(AnyTlsError::Protocol(format!(
            "No address found for {}",
            host
        )));
    }

    // Sort to keep stability across runs (helps caching)
    addresses.sort_unstable_by_key(|addr| match addr {
        IpAddr::V4(ip) => (0, ip.octets().to_vec()),
        IpAddr::V6(ip) => (1, ip.octets().to_vec()),
    });
    addresses.dedup();

    debug!(
        "[DNS] Resolved {} -> {} entries (ttl={}s)",
        host,
        addresses.len(),
        DEFAULT_TTL.as_secs()
    );

    DNS_CACHE.insert(host.to_string(), addresses.clone()).await;
    Ok(addresses
        .into_iter()
        .map(|ip| SocketAddr::new(ip, port))
        .collect())
}

pub async fn set_custom_dns_servers(servers: &[String]) -> Result<()> {
    let mut parsed_servers = Vec::new();
    for raw in servers {
        let socket = parse_dns_server(raw)
            .map_err(|err| AnyTlsError::Config(format!("Invalid DNS server '{}': {}", raw, err)))?;
        parsed_servers.push(socket);
    }

    let mut resolver_guard = DNS_RESOLVER.write().await;

    if parsed_servers.is_empty() {
        *resolver_guard = None;
        DNS_CACHE.clear().await;
        info!("[DNS] Using system DNS resolver");
        return Ok(());
    }

    let resolver_config = resolver_config_for_servers(&parsed_servers);
    let resolver = hickory_resolver::Resolver::builder_with_config(
        resolver_config,
        TokioRuntimeProvider::default(),
    )
    .build()
    .map_err(|err| AnyTlsError::Config(format!("Failed to create custom DNS resolver: {err}")))?;
    *resolver_guard = Some(Arc::new(resolver));
    DNS_CACHE.clear().await;

    info!(
        "[DNS] Custom DNS servers configured: {}",
        parsed_servers
            .iter()
            .map(|addr| addr.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );

    Ok(())
}

fn resolver_config_for_servers(servers: &[SocketAddr]) -> ResolverConfig {
    let name_servers = servers
        .iter()
        .map(|server| {
            let mut udp = ConnectionConfig::udp();
            udp.port = server.port();
            let mut tcp = ConnectionConfig::tcp();
            tcp.port = server.port();
            NameServerConfig::new(server.ip(), true, vec![udp, tcp])
        })
        .collect();

    ResolverConfig::from_name_servers(name_servers)
}

fn parse_dns_server(entry: &str) -> std::io::Result<SocketAddr> {
    let trimmed = entry.trim();
    if trimmed.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "DNS server address is empty",
        ));
    }

    if let Ok(addr) = trimmed.parse::<SocketAddr>() {
        return Ok(addr);
    }

    // Allow IPv6 without port (e.g., "2001:4860:4860::8888")
    if let Ok(ip) = trimmed.parse::<IpAddr>() {
        return Ok(SocketAddr::new(ip, 53));
    }

    // Allow bracketed IPv6 without port (e.g., "[2001:4860:4860::8888]")
    if trimmed.starts_with('[')
        && trimmed.ends_with(']')
        && let Ok(ip) = trimmed[1..trimmed.len() - 1].parse::<IpAddr>()
    {
        return Ok(SocketAddr::new(ip, 53));
    }

    Err(Error::new(
        ErrorKind::InvalidInput,
        format!("invalid DNS server '{}'", entry),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_resolver_config_preserves_server_ports_for_udp_and_tcp() {
        let servers = [
            "1.1.1.1:5353".parse().unwrap(),
            "[2606:4700:4700::1111]:8053".parse().unwrap(),
        ];
        let (_, _, configured) = resolver_config_for_servers(&servers).into_parts();

        assert_eq!(configured.len(), 2);
        for (server, expected_ip, expected_port) in [
            (
                configured[0].clone(),
                "1.1.1.1".parse::<IpAddr>().unwrap(),
                5353,
            ),
            (
                configured[1].clone(),
                "2606:4700:4700::1111".parse::<IpAddr>().unwrap(),
                8053,
            ),
        ] {
            assert_eq!(server.ip, expected_ip);
            assert_eq!(server.connections.len(), 2);
            assert!(
                server
                    .connections
                    .iter()
                    .all(|connection| connection.port == expected_port)
            );
        }
    }

    #[tokio::test]
    async fn cached_addresses_use_each_request_port() {
        let cache = DnsCache::new();
        cache
            .insert(
                "example.test".into(),
                vec![IpAddr::V4("192.0.2.1".parse().unwrap())],
            )
            .await;

        let http = cache.get("example.test", 80).await.unwrap();
        let https = cache.get("example.test", 443).await.unwrap();
        assert_eq!(http[0], "192.0.2.1:80".parse().unwrap());
        assert_eq!(https[0], "192.0.2.1:443".parse().unwrap());
    }

    #[tokio::test]
    async fn cache_rotates_all_addresses_and_keeps_ports() {
        let cache = DnsCache::new();
        cache
            .insert(
                "example.test".into(),
                vec![
                    IpAddr::V4("192.0.2.1".parse().unwrap()),
                    IpAddr::V4("192.0.2.2".parse().unwrap()),
                ],
            )
            .await;

        let first = cache.get("example.test", 8080).await.unwrap();
        let second = cache.get("example.test", 8080).await.unwrap();
        assert_eq!(first[0].ip(), "192.0.2.2".parse::<IpAddr>().unwrap());
        assert_eq!(second[0].ip(), "192.0.2.1".parse::<IpAddr>().unwrap());
        assert!(first.iter().all(|address| address.port() == 8080));
        assert!(second.iter().all(|address| address.port() == 8080));
    }

    #[tokio::test]
    async fn cache_entry_count_is_bounded() {
        let cache = DnsCache::new();
        for index in 0..=MAX_CACHE_ENTRIES {
            cache
                .insert(
                    format!("host-{index}.test"),
                    vec!["127.0.0.1".parse().unwrap()],
                )
                .await;
        }
        assert_eq!(cache.inner.read().await.len(), MAX_CACHE_ENTRIES);
    }
}
