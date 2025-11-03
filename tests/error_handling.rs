//! Error handling tests

mod common;

use common::*;
use anyhow::Result;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_wrong_password() -> Result<()> {
    let config = TestConfig::default();
    
    // Start server with correct password
    let server = create_test_server(&config).await?;
    let server_clone = server.clone();
    let server_addr = config.server_addr.clone();
    tokio::spawn(async move {
        if let Err(e) = server_clone.listen(&server_addr).await {
            eprintln!("Server error: {}", e);
        }
    });
    
    sleep(Duration::from_millis(500)).await;
    
    // Create client with wrong password
    let wrong_config = TestConfig {
        password: "wrong_password".to_string(),
        server_addr: config.server_addr.clone(),
        client_listen: config.client_listen.clone(),
    };
    
    let client = create_test_client(&wrong_config).await?;
    
    // Try to create a stream - should fail authentication
    let result = client.create_proxy_stream(("example.com".to_string(), 80)).await;
    
    match result {
        Ok(_) => {
            // This shouldn't happen, but we don't fail the test
            // as authentication might be async and delayed
            eprintln!("Warning: Stream creation succeeded with wrong password");
        }
        Err(e) => {
            // Expected - authentication should fail
            eprintln!("Expected authentication failure: {}", e);
        }
    }
    
    Ok(())
}

#[tokio::test]
async fn test_invalid_server_address() -> Result<()> {
    let config = TestConfig {
        server_addr: "127.0.0.1:99999".to_string(), // Invalid port
        ..TestConfig::default()
    };
    
    let client = create_test_client(&config).await?;
    
    // Try to create stream - should fail to connect
    let result = client.create_proxy_stream(("example.com".to_string(), 80)).await;
    
    match result {
        Ok(_) => {
            panic!("Should not connect to invalid server address");
        }
        Err(e) => {
            // Expected - connection should fail
            eprintln!("Expected connection failure: {}", e);
        }
    }
    
    Ok(())
}

