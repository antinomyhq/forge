use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

/// Start a temporary local HTTP server to receive OAuth callback
/// Binds to the specified port (default: 3000 for localhost)
/// Returns receiver for the authorization code
pub fn start_callback_server(port: u16) -> anyhow::Result<oneshot::Receiver<String>> {
    // Try to bind to the specified port with SO_REUSEADDR for immediate port reuse
    let listener = TcpListener::bind(format!("127.0.0.1:{port}"))
        .map_err(|e| anyhow::anyhow!("Failed to start callback server on port {port}: {e}"))?;

    // Set SO_REUSEADDR to allow immediate port reuse after server shuts down
    listener.set_nonblocking(false)?;

    let (tx, rx) = oneshot::channel();
    let tx = Arc::new(Mutex::new(Some(tx)));

    // Spawn server in background
    std::thread::spawn(move || {
        tracing::debug!("OAuth callback server started on port {port}");
        if let Err(e) = run_server(listener, tx) {
            tracing::error!("OAuth callback server error: {e}");
        }
        tracing::debug!("OAuth callback server shut down on port {port}");
    });

    Ok(rx)
}

fn run_server(
    listener: TcpListener,
    tx: Arc<Mutex<Option<oneshot::Sender<String>>>>,
) -> anyhow::Result<()> {
    use std::io::{Read, Write};

    // Accept exactly one connection
    let (mut stream, _) = listener.accept()?;

    // Drop the listener immediately to release the port
    drop(listener);

    tracing::debug!("OAuth callback received, processing request");

    // Read the HTTP request
    let mut buffer = [0; 2048];
    let bytes_read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);

    // Extract code from query string
    // Example: GET /?code=abc123&state=xyz HTTP/1.1
    let code = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|path| path.split('?').nth(1))
        .and_then(|query| {
            query.split('&').find_map(|param| {
                let mut parts = param.split('=');
                if parts.next() == Some("code") {
                    parts.next().map(|c| c.to_string())
                } else {
                    None
                }
            })
        });

    // Send success response to browser
    let response = if code.is_some() {
        "HTTP/1.1 200 OK\r\n\
         Content-Type: text/html\r\n\
         Connection: close\r\n\
         \r\n\
         <html>\
         <head>\
         <style>\
         body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; text-align: center; padding-top: 50px; background: #f5f5f5; }\
         .container { background: white; max-width: 500px; margin: 0 auto; padding: 40px; border-radius: 10px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); }\
         h1 { color: #4CAF50; margin-bottom: 20px; }\
         .countdown { font-size: 18px; color: #666; margin: 20px 0; }\
         .close-btn { display: inline-block; padding: 12px 24px; background: #4CAF50; color: white; text-decoration: none; border-radius: 5px; border: none; font-size: 16px; cursor: pointer; margin-top: 20px; }\
         .close-btn:hover { background: #45a049; }\
         </style>\
         </head>\
         <body>\
         <div class='container'>\
         <h1>✓ Authentication Successful</h1>\
         <p>You have successfully authenticated with your provider.</p>\
         <p class='countdown' id='countdown'>This window will close in <span id='timer'>3</span> seconds...</p>\
         <button class='close-btn' onclick='window.close()'>Close Window Now</button>\
         <p style='color: #999; font-size: 14px; margin-top: 20px;'>If the window doesn't close automatically, please close it manually and return to the terminal.</p>\
         </div>\
         <script>\
         let seconds = 3;\
         const timer = document.getElementById('timer');\
         const countdown = setInterval(() => {\
             seconds--;\
             timer.textContent = seconds;\
             if (seconds <= 0) {\
                 clearInterval(countdown);\
                 window.close();\
                 setTimeout(() => {\
                     document.getElementById('countdown').textContent = 'Please close this window manually and return to the terminal.';\
                 }, 500);\
             }\
         }, 1000);\
         </script>\
         </body>\
         </html>"
    } else {
        "HTTP/1.1 400 Bad Request\r\n\
         Content-Type: text/html\r\n\
         Connection: close\r\n\
         \r\n\
         <html>\
         <head>\
         <style>\
         body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; text-align: center; padding-top: 50px; background: #f5f5f5; }\
         .container { background: white; max-width: 500px; margin: 0 auto; padding: 40px; border-radius: 10px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); }\
         h1 { color: #f44336; margin-bottom: 20px; }\
         .close-btn { display: inline-block; padding: 12px 24px; background: #f44336; color: white; text-decoration: none; border-radius: 5px; border: none; font-size: 16px; cursor: pointer; margin-top: 20px; }\
         .close-btn:hover { background: #da190b; }\
         </style>\
         </head>\
         <body>\
         <div class='container'>\
         <h1>✗ Authentication Failed</h1>\
         <p>No authorization code received. Please try again.</p>\
         <button class='close-btn' onclick='window.close()'>Close Window</button>\
         </div>\
         </body>\
         </html>"
    };

    stream.write_all(response.as_bytes())?;
    stream.flush()?;

    // Small delay to ensure browser receives complete response before server shuts
    // down
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Send code to receiver if available
    if let Some(code) = code {
        tracing::debug!("Sending authorization code to CLI");
        if let Ok(mut tx_guard) = tx.lock()
            && let Some(sender) = tx_guard.take()
        {
            let _ = sender.send(code);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::net::TcpStream;

    use super::*;

    #[test]
    fn test_start_callback_server() {
        // Start server on port 0 (OS assigns random port)
        let mut rx = start_callback_server(0).expect("Failed to start callback server");

        // Server should be running and receiver should be waiting
        assert!(rx.try_recv().is_err()); // No code yet
    }

    #[test]
    fn test_callback_server_receives_code() {
        // Start server on a random port
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener); // Release the port

        let mut rx = start_callback_server(port).expect("Failed to start callback server");

        // Give server time to start
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Send a request with an authorization code
        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
            .expect("Failed to connect to callback server");

        let request = "GET /?code=test_auth_code_123&state=xyz HTTP/1.1\r\nHost: localhost\r\n\r\n";
        stream.write_all(request.as_bytes()).unwrap();
        stream.flush().unwrap();

        // Wait for response
        std::thread::sleep(std::time::Duration::from_millis(300));

        // Verify code was received
        let code = rx
            .try_recv()
            .expect("Should have received authorization code");
        assert_eq!(code, "test_auth_code_123");

        // Verify port is released - try to bind again
        std::thread::sleep(std::time::Duration::from_millis(100));
        let rebind = std::net::TcpListener::bind(format!("127.0.0.1:{}", port));
        assert!(
            rebind.is_ok(),
            "Port should be released after server shuts down"
        );
    }
}
