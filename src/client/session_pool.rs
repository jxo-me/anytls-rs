//! Session pool for connection reuse

use crate::session::Session;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};

/// SessionPool manages idle sessions for reuse
pub struct SessionPool {
    idle_sessions: Arc<RwLock<BTreeMap<u64, PooledSession>>>,
    min_idle: usize,
    idle_timeout: Duration,
}

struct PooledSession {
    session: Arc<Session>,
    idle_since: Instant,
    #[allow(dead_code)]
    seq: u64,
}

impl SessionPool {
    /// Create a new session pool
    pub fn new() -> Self {
        Self {
            idle_sessions: Arc::new(RwLock::new(BTreeMap::new())),
            min_idle: 5,
            idle_timeout: Duration::from_secs(30),
        }
    }

    /// Get an idle session (returns the most recent one)
    pub async fn get_idle_session(&self) -> Option<Arc<Session>> {
        let mut sessions = self.idle_sessions.write().await;
        
        if sessions.is_empty() {
            return None;
        }
        
        // Get the most recent session (last in BTreeMap)
        // Use last_key_value to get the key-value pair
        if let Some((key, _)) = sessions.last_key_value() {
            let key_clone = *key;
            if let Some(pooled) = sessions.remove(&key_clone) {
                return Some(pooled.session);
            }
        }
        
        None
    }

    /// Add a session to the idle pool
    pub async fn add_idle_session(&self, session: Arc<Session>) {
        let seq = session.seq();
        let key = u64::MAX - seq; // Reverse order for max priority
        
        let pooled = PooledSession {
            session,
            idle_since: Instant::now(),
            seq,
        };
        
        let mut sessions = self.idle_sessions.write().await;
        sessions.insert(key, pooled);
    }

    /// Clean up expired idle sessions
    pub async fn cleanup_expired(&self) {
        let now = Instant::now();
        let expiry_time = now - self.idle_timeout;
        
        let mut sessions = self.idle_sessions.write().await;
        let mut to_remove = Vec::new();
        
        let mut active_count = 0;
        for (key, pooled) in sessions.iter() {
            if pooled.idle_since > expiry_time {
                active_count += 1;
                continue;
            }
            
            // Keep at least min_idle sessions
            if active_count < self.min_idle {
                active_count += 1;
                continue;
            }
            
            to_remove.push(*key);
        }
        
        // Remove expired sessions
        for key in to_remove {
            sessions.remove(&key);
        }
    }
}

