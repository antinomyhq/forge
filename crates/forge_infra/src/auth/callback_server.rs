use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use tokio::task::spawn_blocking;

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

    // Spawn server in background using tokio
    spawn_blocking(move || {
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
        r#"HTTP/1.1 200 OK
Content-Type: text/html
Connection: close

<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Authentication Successful</title>
    <style>
        * {
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }
        
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
            background: linear-gradient(135deg, #e0e7ff 0%, #f0f9ff 100%);
            min-height: 100vh;
            display: flex;
            align-items: center;
            justify-content: center;
            padding: 20px;
        }
        
        .container {
            background: white;
            border-radius: 24px;
            box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.25);
            max-width: 480px;
            width: 100%;
            padding: 48px 40px;
            text-align: center;
            animation: slideUp 0.5s ease-out;
        }
        
        @keyframes slideUp {
            from {
                opacity: 0;
                transform: translateY(30px);
            }
            to {
                opacity: 1;
                transform: translateY(0);
            }
        }
        
        .icon {
            width: 80px;
            height: 80px;
            background: linear-gradient(135deg, #10b981 0%, #059669 100%);
            border-radius: 50%;
            display: flex;
            align-items: center;
            justify-content: center;
            margin: 0 auto 24px;
            animation: scaleIn 0.5s ease-out 0.2s both;
        }
        
        @keyframes scaleIn {
            from {
                transform: scale(0);
            }
            to {
                transform: scale(1);
            }
        }
        
        .icon svg {
            width: 40px;
            height: 40px;
            fill: white;
        }
        
        h1 {
            font-size: 28px;
            font-weight: 700;
            color: #1f2937;
            margin-bottom: 12px;
        }
        
        .subtitle {
            font-size: 16px;
            color: #6b7280;
            line-height: 1.6;
            margin-bottom: 32px;
        }
        
        .info-box {
            background: #f9fafb;
            border: 1px solid #e5e7eb;
            border-radius: 12px;
            padding: 20px;
            margin-bottom: 24px;
        }
        
        .info-title {
            font-size: 14px;
            font-weight: 600;
            color: #374151;
            margin-bottom: 8px;
        }
        
        .info-text {
            font-size: 14px;
            color: #6b7280;
            line-height: 1.5;
        }
        
        .steps {
            text-align: left;
            margin-top: 16px;
        }
        
        .step {
            display: flex;
            align-items: flex-start;
            margin-bottom: 8px;
            font-size: 13px;
            color: #6b7280;
        }
        
        .step-number {
            display: inline-flex;
            align-items: center;
            justify-content: center;
            width: 20px;
            height: 20px;
            background: #10b981;
            color: white;
            border-radius: 50%;
            font-size: 11px;
            font-weight: 600;
            margin-right: 8px;
            flex-shrink: 0;
        }
        
        .divider {
            height: 1px;
            background: #e5e7eb;
            margin: 24px 0;
        }
        
        .footer-text {
            font-size: 12px;
            color: #9ca3af;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="icon">
            <svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
                <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41L9 16.17z"/>
            </svg>
        </div>
        <h1>Authentication Successful</h1>
        <p class="subtitle">You have successfully authenticated with your provider.</p>
        
        <div class="info-box">
            <div class="info-title">What's Next?</div>
            <div class="steps">
                <div class="step">
                    <span class="step-number">1</span>
                    <span>Return to your terminal window</span>
                </div>
                <div class="step">
                    <span class="step-number">2</span>
                    <span>Your authentication is now complete</span>
                </div>
                <div class="step">
                    <span class="step-number">3</span>
                    <span>You can close this browser tab</span>
                </div>
            </div>
        </div>
        
        <div class="divider"></div>
        <p class="footer-text">You can safely close this tab and return to your terminal.</p>
    </div>
</body>
</html>"#
    } else {
        r#"HTTP/1.1 400 Bad Request
Content-Type: text/html
Connection: close

<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Authentication Failed</title>
    <style>
        * {
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }
        
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
            background: linear-gradient(135deg, #fee2e2 0%, #fef2f2 100%);
            min-height: 100vh;
            display: flex;
            align-items: center;
            justify-content: center;
            padding: 20px;
        }
        
        .container {
            background: white;
            border-radius: 24px;
            box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.25);
            max-width: 480px;
            width: 100%;
            padding: 48px 40px;
            text-align: center;
            animation: slideUp 0.5s ease-out;
        }
        
        @keyframes slideUp {
            from {
                opacity: 0;
                transform: translateY(30px);
            }
            to {
                opacity: 1;
                transform: translateY(0);
            }
        }
        
        .icon {
            width: 80px;
            height: 80px;
            background: linear-gradient(135deg, #ef4444 0%, #dc2626 100%);
            border-radius: 50%;
            display: flex;
            align-items: center;
            justify-content: center;
            margin: 0 auto 24px;
            animation: scaleIn 0.5s ease-out 0.2s both;
        }
        
        @keyframes scaleIn {
            from {
                transform: scale(0);
            }
            to {
                transform: scale(1);
            }
        }
        
        .icon svg {
            width: 40px;
            height: 40px;
            fill: white;
        }
        
        h1 {
            font-size: 28px;
            font-weight: 700;
            color: #1f2937;
            margin-bottom: 12px;
        }
        
        .subtitle {
            font-size: 16px;
            color: #6b7280;
            line-height: 1.6;
            margin-bottom: 32px;
        }
        
        .info-box {
            background: #fef2f2;
            border: 1px solid #fecaca;
            border-radius: 12px;
            padding: 20px;
            margin-bottom: 24px;
        }
        
        .info-title {
            font-size: 14px;
            font-weight: 600;
            color: #991b1b;
            margin-bottom: 8px;
        }
        
        .info-text {
            font-size: 14px;
            color: #b91c1c;
            line-height: 1.5;
        }
        
        .divider {
            height: 1px;
            background: #e5e7eb;
            margin: 24px 0;
        }
        
        .footer-text {
            font-size: 12px;
            color: #9ca3af;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="icon">
            <svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
                <path d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"/>
            </svg>
        </div>
        <h1>Authentication Failed</h1>
        <p class="subtitle">No authorization code received. Please try again.</p>
        
        <div class="info-box">
            <div class="info-title">What Happened?</div>
            <div class="info-text">
                The authentication process did not complete successfully. This could be due to a timeout or an interrupted connection.
            </div>
        </div>
        
        <div class="divider"></div>
        <p class="footer-text">Please close this tab and restart the authentication process from your terminal.</p>
    </div>
</body>
</html>"#
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
