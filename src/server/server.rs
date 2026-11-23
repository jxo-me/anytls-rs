//! AnyTLS Server implementation

use crate::padding::PaddingFactory;
use crate::server::handler::{StreamHandler, TcpProxyHandler};
use crate::session::Session;
use crate::util::{
    AnyTlsError, Result, StringMap, authenticate_client, configure_tcp_stream, hash_password,
};
use std::future::Future;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::timeout;
use tokio_rustls::TlsAcceptor;
use tracing::{Instrument, Span, field, info_span};

/// Bound unauthenticated connections so slow handshakes cannot spawn unlimited tasks.
const MAX_CONCURRENT_HANDSHAKES: usize = 128;
const MAX_ACTIVE_CONNECTIONS: usize = 1024;
const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
const CLIENT_AUTH_TIMEOUT: Duration = Duration::from_secs(15);
const ACCEPT_BACKOFF_MIN: Duration = Duration::from_millis(100);
const ACCEPT_BACKOFF_MAX: Duration = Duration::from_secs(1);

/// Server manages AnyTLS server connections
pub struct Server {
    password_hash: [u8; 32],
    tls_config: Arc<RwLock<Arc<TlsAcceptor>>>,
    padding: Arc<PaddingFactory>,
    on_new_stream: Option<Arc<dyn Fn(Arc<crate::session::Stream>) + Send + Sync + 'static>>,
    server_settings: Option<StringMap>,
}

struct ConnectionContext {
    tls_config: Arc<TlsAcceptor>,
    password_hash: [u8; 32],
    padding: Arc<PaddingFactory>,
    on_new_stream: Option<Arc<dyn Fn(Arc<crate::session::Stream>) + Send + Sync + 'static>>,
    server_settings: Option<StringMap>,
    active_slots: Arc<Semaphore>,
}

impl Server {
    /// Create a new server
    pub fn new(
        password: &str,
        tls_config: Arc<TlsAcceptor>,
        padding: Arc<PaddingFactory>,
        server_settings: Option<StringMap>,
    ) -> Self {
        let password_hash = hash_password(password);

        Self {
            password_hash,
            tls_config: Arc::new(RwLock::new(tls_config)),
            padding,
            on_new_stream: None,
            server_settings,
        }
    }

    /// Create a new server with reloadable TLS config
    pub fn new_with_reloadable_tls(
        password: &str,
        tls_config: Arc<RwLock<Arc<TlsAcceptor>>>,
        padding: Arc<PaddingFactory>,
        server_settings: Option<StringMap>,
    ) -> Self {
        let password_hash = hash_password(password);

        Self {
            password_hash,
            tls_config,
            padding,
            on_new_stream: None,
            server_settings,
        }
    }

    /// Set callback for new streams
    pub fn with_stream_handler<F>(mut self, callback: F) -> Self
    where
        F: Fn(Arc<crate::session::Stream>) + Send + Sync + 'static,
    {
        self.on_new_stream = Some(Arc::new(callback));
        self
    }

    /// Start the server and listen for connections
    pub async fn listen(&self, addr: &str) -> Result<()> {
        let listener = TcpListener::bind(addr).await?;
        let handshake_slots = Arc::new(Semaphore::new(MAX_CONCURRENT_HANDSHAKES));
        let active_slots = Arc::new(Semaphore::new(MAX_ACTIVE_CONNECTIONS));
        let mut accept_backoff = ACCEPT_BACKOFF_MIN;

        tracing::info!("[Server] Listening on {}", addr);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    accept_backoff = ACCEPT_BACKOFF_MIN;
                    let Some(handshake_permit) = try_acquire_handshake_permit(&handshake_slots)
                    else {
                        tracing::debug!(%addr, "Dropping connection: handshake limit reached");
                        drop(stream);
                        continue;
                    };
                    let tls_config = self.tls_config.read().unwrap().clone();
                    let password_hash = self.password_hash;
                    let padding = Arc::clone(&self.padding);
                    let on_new_stream = self.on_new_stream.clone();
                    let server_settings = self.server_settings.clone();
                    let active_slots = Arc::clone(&active_slots);
                    let context = ConnectionContext {
                        tls_config,
                        password_hash,
                        padding,
                        on_new_stream,
                        server_settings,
                        active_slots,
                    };
                    let span = info_span!(
                        "anytls.connection",
                        peer_addr = %addr,
                        session_id = field::Empty
                    );

                    tokio::spawn(
                        async move {
                            if let Err(e) =
                                handle_connection(stream, handshake_permit, context).await
                            {
                                tracing::error!("[Server] Connection error: {}", e);
                            }
                        }
                        .instrument(span),
                    );
                }
                Err(e) => {
                    tracing::error!("[Server] Accept error: {}", e);
                    tokio::time::sleep(accept_backoff).await;
                    accept_backoff = next_accept_backoff(accept_backoff);
                }
            }
        }
    }
}

fn try_acquire_handshake_permit(slots: &Arc<Semaphore>) -> Option<OwnedSemaphorePermit> {
    Arc::clone(slots).try_acquire_owned().ok()
}

fn next_accept_backoff(current: Duration) -> Duration {
    (current * 2).min(ACCEPT_BACKOFF_MAX)
}

async fn with_handshake_timeout<F, T, E>(duration: Duration, future: F, stage: &str) -> Result<T>
where
    F: Future<Output = std::result::Result<T, E>>,
    E: Into<AnyTlsError>,
{
    timeout(duration, future)
        .await
        .map_err(|_| {
            AnyTlsError::Protocol(format!("{} timed out after {}s", stage, duration.as_secs()))
        })?
        .map_err(Into::into)
}

/// Handle a single TCP connection
async fn handle_connection(
    tcp_stream: tokio::net::TcpStream,
    handshake_permit: OwnedSemaphorePermit,
    context: ConnectionContext,
) -> Result<()> {
    let ConnectionContext {
        tls_config,
        password_hash,
        padding,
        on_new_stream,
        server_settings,
        active_slots,
    } = context;
    let peer_addr = tcp_stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    configure_tcp_stream(&tcp_stream, &peer_addr);
    let handshake_span = info_span!(
        "anytls.handshake",
        peer_addr = %peer_addr,
        session_id = field::Empty,
        tls_version = field::Empty,
        cipher_suite = field::Empty
    );
    let _handshake_guard = handshake_span.enter();
    tracing::info!("[Server] New connection from {}", peer_addr);
    // Perform TLS handshake
    tracing::debug!("[Server] Starting TLS handshake");
    let tls_stream = with_handshake_timeout(
        TLS_HANDSHAKE_TIMEOUT,
        async {
            tls_config
                .accept(tcp_stream)
                .await
                .map_err(|e| AnyTlsError::Tls(format!("TLS handshake failed: {}", e)))
        },
        "TLS handshake",
    )
    .await
    .map_err(|error| {
        tracing::error!("[Server] TLS handshake failed: {}", error);
        error
    })?;
    tracing::debug!("[Server] TLS handshake successful");
    let (_, server_connection) = tls_stream.get_ref();
    if let Some(protocol) = server_connection.protocol_version() {
        handshake_span.record("tls_version", field::display(format!("{:?}", protocol)));
    }
    if let Some(suite) = server_connection.negotiated_cipher_suite() {
        handshake_span.record(
            "cipher_suite",
            field::display(format!("{:?}", suite.suite())),
        );
    }

    // Authenticate client
    tracing::debug!("[Server] Authenticating client");
    let (mut reader, writer) = tokio::io::split(tls_stream);
    with_handshake_timeout(
        CLIENT_AUTH_TIMEOUT,
        authenticate_client(&mut reader, &password_hash, &padding),
        "Client authentication",
    )
    .await?;
    drop(handshake_permit);
    let active_permit = try_acquire_handshake_permit(&active_slots)
        .ok_or_else(|| AnyTlsError::Protocol("maximum active connection limit reached".into()))?;
    tracing::debug!("[Server] Client authenticated");

    // Create callback channel for new streams
    let (stream_callback_tx, mut stream_callback_rx) =
        tokio::sync::mpsc::unbounded_channel::<Arc<crate::session::Stream>>();

    // Create server session
    let mut session = Session::new_server(reader, writer, padding);
    session.set_server_settings(server_settings.clone());

    // Set callback channel in session
    session.set_stream_callback(stream_callback_tx);

    let session = Arc::new(session);
    let session_id = session.id();
    Span::current().record("session_id", session_id);
    handshake_span.record("session_id", field::display(session_id));

    tracing::info!(
        session_id = session_id,
        peer_addr = %peer_addr,
        "[Server] Session {} created",
        session_id
    );

    // Handle new streams in a task
    if let Some(callback) = on_new_stream {
        tracing::debug!("[Server] Using custom stream callback");
        let dispatcher_session = Arc::clone(&session);
        tokio::spawn(async move {
            let mut handlers = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    _ = dispatcher_session.closed() => break,
                    stream = stream_callback_rx.recv() => {
                        let Some(stream) = stream else { break };
                        let callback = Arc::clone(&callback);
                        let stream_id = stream.id();
                        let span = info_span!("anytls.stream.callback", session_id, stream_id);
                        handlers.spawn(async move { callback(stream) }.instrument(span));
                    }
                    result = handlers.join_next(), if !handlers.is_empty() => {
                        if let Some(Err(error)) = result {
                            tracing::error!(session_id, "[Server] Stream callback task failed: {}", error);
                        }
                    }
                }
            }
            handlers.abort_all();
            while handlers.join_next().await.is_some() {}
        });
    } else {
        // Use default TCP proxy handler if no callback is provided
        tracing::debug!("[Server] Using default TCP proxy handler");
        let session_for_handler = Arc::clone(&session);
        tokio::spawn(async move {
            let mut handlers = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    _ = session_for_handler.closed() => break,
                    stream = stream_callback_rx.recv() => {
                        let Some(stream) = stream else { break };
                        let session = Arc::clone(&session_for_handler);
                        let stream_id = stream.id();
                        let span = info_span!("anytls.stream.proxy", session_id = session.id(), stream_id);
                        handlers.spawn(async move {
                            if let Err(error) = TcpProxyHandler::new().handle_stream(stream, session).await {
                                tracing::error!("[Proxy] Handler error: {}", error);
                            }
                        }.instrument(span));
                    }
                    result = handlers.join_next(), if !handlers.is_empty() => {
                        if let Some(Err(error)) = result {
                            tracing::error!(session_id, "[Server] Stream handler task failed: {}", error);
                        }
                    }
                }
            }
            handlers.abort_all();
            while handlers.join_next().await.is_some() {}
        });
    }

    // Supervise the transport loops together. If either side exits, close the
    // session and cancel its sibling so the authenticated permit is released.
    let mut transport_tasks = tokio::task::JoinSet::new();
    let recv_session = Arc::clone(&session);
    let recv_span =
        info_span!("anytls.session.recv_loop", session_id = session_id, peer_addr = %peer_addr);
    transport_tasks.spawn(async move { recv_session.recv_loop().await }.instrument(recv_span));
    let process_session = Arc::clone(&session);
    let process_span = info_span!("anytls.session.process_stream_data", session_id = session_id, peer_addr = %peer_addr);
    transport_tasks
        .spawn(async move { process_session.process_stream_data().await }.instrument(process_span));
    tokio::spawn(async move {
        let _active_permit = active_permit;
        if let Some(result) = transport_tasks.join_next().await {
            match result {
                Err(error) => tracing::error!(
                    session_id,
                    "[Server] Session transport task panicked: {}",
                    error
                ),
                Ok(Err(error)) => {
                    tracing::debug!(session_id, "[Server] Session transport ended: {}", error)
                }
                Ok(Ok(())) => {}
            }
        }
        let _ = session.close().await;
        transport_tasks.abort_all();
        while transport_tasks.join_next().await.is_some() {}
    });

    tracing::debug!("[Server] Connection handler setup complete");

    // Wait for connection to close
    // The connection will be managed by the spawned tasks
    // In a production implementation, we'd wait for the session to close

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn handshake_admission_is_bounded_and_released() {
        let slots = Arc::new(Semaphore::new(1));
        let permit = try_acquire_handshake_permit(&slots).unwrap();
        assert!(try_acquire_handshake_permit(&slots).is_none());

        drop(permit);
        assert!(try_acquire_handshake_permit(&slots).is_some());
    }

    #[test]
    fn active_connection_admission_is_bounded_and_released() {
        let slots = Arc::new(Semaphore::new(1));
        let permit = try_acquire_handshake_permit(&slots).unwrap();
        assert!(try_acquire_handshake_permit(&slots).is_none());
        drop(permit);
        assert!(try_acquire_handshake_permit(&slots).is_some());
    }

    #[test]
    fn accept_backoff_grows_to_a_bounded_maximum() {
        let mut backoff = ACCEPT_BACKOFF_MIN;
        for expected_ms in [200, 400, 800, 1000, 1000] {
            backoff = next_accept_backoff(backoff);
            assert_eq!(backoff, Duration::from_millis(expected_ms));
        }
    }

    #[tokio::test]
    async fn stalled_handshake_times_out() {
        let error = with_handshake_timeout(
            Duration::from_millis(1),
            std::future::pending::<io::Result<()>>(),
            "test handshake",
        )
        .await
        .unwrap_err();

        assert!(matches!(error, AnyTlsError::Protocol(message) if message.contains("timed out")));
    }
}
