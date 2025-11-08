//! AnyTLS Server implementation

use crate::padding::PaddingFactory;
use crate::server::handler::{StreamHandler, TcpProxyHandler};
use crate::session::Session;
use crate::util::{AnyTlsError, Result, StringMap, authenticate_client, hash_password};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tracing::{Instrument, Span, field, info_span};

/// Server manages AnyTLS server connections
pub struct Server {
    password_hash: [u8; 32],
    tls_config: Arc<TlsAcceptor>,
    padding: Arc<PaddingFactory>,
    on_new_stream: Option<Arc<dyn Fn(Arc<crate::session::Stream>) + Send + Sync + 'static>>,
    server_settings: Option<StringMap>,
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

        tracing::info!("[Server] Listening on {}", addr);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    let tls_config = Arc::clone(&self.tls_config);
                    let password_hash = self.password_hash;
                    let padding = Arc::clone(&self.padding);
                    let on_new_stream = self.on_new_stream.clone();
                    let server_settings = self.server_settings.clone();
                    let span = info_span!(
                        "anytls.connection",
                        peer_addr = %addr,
                        session_id = field::Empty
                    );

                    tokio::spawn(
                        async move {
                            if let Err(e) = handle_connection(
                                stream,
                                tls_config,
                                password_hash,
                                padding,
                                on_new_stream,
                                server_settings,
                            )
                            .await
                            {
                                tracing::error!("[Server] Connection error: {}", e);
                            }
                        }
                        .instrument(span),
                    );
                }
                Err(e) => {
                    tracing::error!("[Server] Accept error: {}", e);
                }
            }
        }
    }
}

/// Handle a single TCP connection
async fn handle_connection(
    tcp_stream: tokio::net::TcpStream,
    tls_config: Arc<TlsAcceptor>,
    password_hash: [u8; 32],
    padding: Arc<PaddingFactory>,
    on_new_stream: Option<Arc<dyn Fn(Arc<crate::session::Stream>) + Send + Sync + 'static>>,
    server_settings: Option<StringMap>,
) -> Result<()> {
    let peer_addr = tcp_stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    tracing::info!("[Server] 🔌 New connection from {}", peer_addr);
    // Perform TLS handshake
    tracing::info!("[Server] 🔐 Starting TLS handshake");
    let tls_stream = tls_config.accept(tcp_stream).await.map_err(|e| {
        tracing::error!("[Server] ❌ TLS handshake failed: {}", e);
        AnyTlsError::Tls(format!("TLS handshake failed: {}", e))
    })?;
    tracing::info!("[Server] ✅ TLS handshake successful");

    // Authenticate client
    tracing::info!("[Server] 🔐 Authenticating client");
    let (mut reader, writer) = tokio::io::split(tls_stream);
    authenticate_client(&mut reader, &password_hash, &padding).await?;
    tracing::info!("[Server] ✅ Client authenticated");

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
    Span::current().record("session_id", &session_id);

    tracing::info!(
        session_id = session_id,
        peer_addr = %peer_addr,
        "[Server] 📋 Session created, setting up handlers"
    );

    // Handle new streams in a task
    if let Some(callback) = on_new_stream {
        tracing::debug!("[Server] Using custom stream callback");
        tokio::spawn(async move {
            while let Some(stream) = stream_callback_rx.recv().await {
                tracing::debug!(
                    "[Server] Received stream {} in custom callback",
                    stream.id()
                );
                // Spawn a new task for each stream to handle it asynchronously
                let stream_clone = Arc::clone(&stream);
                let callback_clone = Arc::clone(&callback);
                let stream_id = stream_clone.id();
                let stream_span =
                    info_span!("anytls.stream.callback", session_id = session_id, stream_id);
                tokio::spawn(
                    async move {
                        callback_clone(stream_clone);
                    }
                    .instrument(stream_span),
                );
            }
        });
    } else {
        // Use default TCP proxy handler if no callback is provided
        tracing::debug!("[Server] Using default TCP proxy handler");
        let session_for_handler = Arc::clone(&session);
        tokio::spawn(async move {
            while let Some(stream) = stream_callback_rx.recv().await {
                tracing::debug!(
                    "[Server] Received stream {} for default handler",
                    stream.id()
                );
                let stream_clone = Arc::clone(&stream);
                let session_clone = Arc::clone(&session_for_handler);
                // Create a new handler instance for each stream (TcpProxyHandler is small and stateless)
                let handler = TcpProxyHandler::new();
                let stream_id = stream_clone.id();
                let stream_span = info_span!(
                    "anytls.stream.proxy",
                    session_id = session_clone.id(),
                    stream_id
                );
                tokio::spawn(
                    async move {
                        if let Err(e) = handler.handle_stream(stream_clone, session_clone).await {
                            tracing::error!("[Proxy] Handler error: {}", e);
                        }
                    }
                    .instrument(stream_span),
                );
            }
        });
    }

    // Start receive loop
    tracing::info!("[Server] 🚀 Starting receive loop");
    let session_clone = Arc::clone(&session);
    let recv_span = info_span!(
        "anytls.session.recv_loop",
        session_id = session_clone.id(),
        peer_addr = %peer_addr
    );
    tokio::spawn(
        async move {
            tracing::info!("[Server] ✅ recv_loop task spawned! Starting server receive loop");
            match session_clone.recv_loop().await {
                Ok(()) => {
                    tracing::info!("[Server] recv_loop task completed normally");
                }
                Err(AnyTlsError::Io(e)) => {
                    // Check if this is a close_notify error (normal connection close)
                    let error_msg = e.to_string();
                    if error_msg.contains("close_notify")
                        || error_msg.contains("unexpected EOF")
                        || e.kind() == std::io::ErrorKind::UnexpectedEof
                    {
                        tracing::debug!(
                            "[Server] recv_loop task ended: Connection closed by client (no close_notify) - this is normal"
                        );
                    } else {
                        tracing::error!("[Server] recv_loop task error: {}", e);
                    }
                }
                Err(e) => {
                    tracing::error!("[Server] recv_loop task error: {}", e);
                }
            }
        }
        .instrument(recv_span),
    );

    // Start stream data processing
    tracing::info!("[Server] 🚀 Starting stream data processing");
    let session_clone = Arc::clone(&session);
    let process_span = info_span!(
        "anytls.session.process_stream_data",
        session_id = session_clone.id(),
        peer_addr = %peer_addr
    );
    tokio::spawn(
        async move {
            tracing::info!(
                "[Server] ✅ process_stream_data task spawned! Starting server stream data processing"
            );
            if let Err(e) = session_clone.process_stream_data().await {
                tracing::error!("[Server] process_stream_data task error: {}", e);
            } else {
                tracing::info!("[Server] process_stream_data task completed normally");
            }
        }
        .instrument(process_span),
    );

    tracing::debug!("[Server] Connection handler setup complete");

    // Wait for connection to close
    // The connection will be managed by the spawned tasks
    // In a production implementation, we'd wait for the session to close

    Ok(())
}
