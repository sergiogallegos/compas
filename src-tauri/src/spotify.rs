//! Spotify OAuth helper (desktop). The frontend runs the Authorization Code + PKCE
//! flow (no client secret); Rust only does the two things the webview can't:
//!   * open the system browser at the authorize URL (via tauri-plugin-opener, frontend-side)
//!   * catch the loopback redirect on 127.0.0.1 and hand the `code` back as an event
//!
//! Redirect URI to register in the Spotify dashboard: http://127.0.0.1:14565/callback

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Fixed loopback port — must match the redirect URI registered in the Spotify app.
pub const REDIRECT_PORT: u16 = 14565;
const LISTEN_TIMEOUT_SECS: u64 = 180;

#[derive(Serialize, Clone)]
struct CodeEvent {
    code: Option<String>,
    error: Option<String>,
}

/// Start a one-shot loopback listener for the OAuth redirect. Emits `spotify:code`
/// with the authorization code (or an error) when the browser redirects back.
#[tauri::command]
pub fn spotify_listen(app: AppHandle) -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", REDIRECT_PORT))
        .map_err(|e| format!("could not bind 127.0.0.1:{REDIRECT_PORT}: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;

    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(LISTEN_TIMEOUT_SECS);
        loop {
            if Instant::now() > deadline {
                let _ = app.emit(
                    "spotify:code",
                    CodeEvent {
                        code: None,
                        error: Some("login timed out".into()),
                    },
                );
                break;
            }
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buf = [0u8; 4096];
                    let n = stream.read(&mut buf).unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let first = req.lines().next().unwrap_or("");
                    let (code, error) = parse_callback(first);

                    let body = "<!doctype html><html><body style=\"font-family:system-ui;background:#16161d;color:#ededf2;display:grid;place-items:center;height:100vh;margin:0\"><div style=\"text-align:center\"><h2 style=\"color:#ff2e7e\">compas</h2><p>Spotify connected — you can close this tab and return to compas.</p></div></body></html>";
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(resp.as_bytes());
                    let _ = app.emit("spotify:code", CodeEvent { code, error });
                    break;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(120));
                }
                Err(e) => {
                    let _ = app.emit(
                        "spotify:code",
                        CodeEvent {
                            code: None,
                            error: Some(e.to_string()),
                        },
                    );
                    break;
                }
            }
        }
    });

    Ok(REDIRECT_PORT)
}

/// Parse the request line `GET /callback?code=...&state=... HTTP/1.1`.
fn parse_callback(line: &str) -> (Option<String>, Option<String>) {
    let path = line.split_whitespace().nth(1).unwrap_or("");
    let query = path.split('?').nth(1).unwrap_or("");
    let mut code = None;
    let mut error = None;
    for kv in query.split('&') {
        let mut it = kv.splitn(2, '=');
        match (it.next(), it.next()) {
            (Some("code"), Some(v)) => code = Some(v.to_string()),
            (Some("error"), Some(v)) => error = Some(v.to_string()),
            _ => {}
        }
    }
    (code, error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_code_from_callback() {
        let (code, err) = parse_callback("GET /callback?code=abc123&state=xyz HTTP/1.1");
        assert_eq!(code.as_deref(), Some("abc123"));
        assert!(err.is_none());
    }

    #[test]
    fn parses_error_from_callback() {
        let (code, err) = parse_callback("GET /callback?error=access_denied HTTP/1.1");
        assert!(code.is_none());
        assert_eq!(err.as_deref(), Some("access_denied"));
    }
}
