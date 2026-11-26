#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    unused_crate_dependencies
)]

//! `current_user`: the one GitHub call that tests a token rather than using it.
//!
//! Nothing here reaches the real GitHub API. Each test binds `127.0.0.1:0` — the
//! port is the kernel's, so no fixed port is ever taken — and points a
//! `GitHubClient` at it with `with_api_base`. The stub is hand-rolled on
//! `tokio::net::TcpListener` because the crate has no mock-HTTP dependency and
//! this packet adds none. The token is the literal `test-token`, so a leak into
//! an error message is visible rather than plausible.
//!
//! What these pin: a working token reports the account's login and name and the
//! request carries the token as a bearer header; a rejected token is
//! `GitError::AuthRequired` and not a formatted HTTP error, so the caller can
//! tell "that token is wrong" from "GitHub said something else"; and a body that
//! is not a user is a decode error, not a silent empty account.

use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use warsha_git::{GitError, GitHubClient};

/// The one credential these tests ever use.
const TOKEN: &str = "test-token";

/// One request as the stub read it off the wire.
#[derive(Debug, Clone)]
struct Captured {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
}

impl Captured {
    /// Case-insensitive header lookup, the way HTTP matches names.
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// Bind a loopback stub that answers exactly one request with `status` and `body`.
///
/// Returns the client base URL and the transcript the stub recorded. The stub
/// records the request *before* it writes the response, so a returned client call
/// guarantees the transcript is populated.
async fn spawn_stub(status: u16, body: &'static str) -> (String, Arc<Mutex<Vec<Captured>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("an ephemeral loopback port");
    let address = listener.local_addr().expect("the bound address");
    let seen: Arc<Mutex<Vec<Captured>>> = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&seen);

    tokio::spawn(async move {
        // The stub ends when the test's runtime does; the handle is not joined and
        // never needs to be, because every test makes exactly one request.
        let Ok((mut socket, _)) = listener.accept().await else {
            return;
        };
        let Some(request) = read_request(&mut socket).await else {
            return;
        };
        if let Ok(mut transcript) = recorded.lock() {
            transcript.push(request);
        }

        let reason = match status {
            200 => "OK",
            401 => "Unauthorized",
            403 => "Forbidden",
            _ => "OK",
        };
        let response = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = socket.write_all(response.as_bytes()).await;
        let _ = socket.flush().await;
        let _ = socket.shutdown().await;
    });

    (format!("http://{address}"), seen)
}

/// Read one HTTP request off the socket: the request line and headers.
async fn read_request(socket: &mut tokio::net::TcpStream) -> Option<Captured> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];

    let header_end = loop {
        if let Some(position) = find(&buffer, b"\r\n\r\n") {
            break position + 4;
        }
        match socket.read(&mut chunk).await {
            Ok(0) => return None,
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
            Err(_) => return None,
        }
    };

    let head = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let mut request_line = lines.next()?.split_whitespace();
    let method = request_line.next()?.to_string();
    let path = request_line.next()?.to_string();

    let headers = lines
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (key, value) = line.split_once(':')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect::<Option<Vec<_>>>()?;

    Some(Captured {
        method,
        path,
        headers,
    })
}

/// First index of `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// A client pointed at the stub.
fn client_for(base: &str) -> GitHubClient {
    GitHubClient::new(TOKEN)
        .expect("a non-empty token")
        .with_api_base(base)
}

/// The single request the stub saw, once the client call has returned.
fn captured(seen: &Arc<Mutex<Vec<Captured>>>) -> Captured {
    let transcript = seen.lock().expect("the transcript lock");
    transcript
        .first()
        .cloned()
        .expect("the stub recorded one request")
}

#[tokio::test]
async fn a_working_token_reports_the_account_it_belongs_to() {
    let (base, seen) = spawn_stub(200, r#"{"login":"octo-mock","name":"Octo Mock"}"#).await;

    let user = client_for(&base)
        .current_user()
        .await
        .expect("the account behind a working token");

    assert_eq!(user.login, "octo-mock");
    assert_eq!(user.name.as_deref(), Some("Octo Mock"));

    // `GET /user` under the API base, with the token, exactly as the REST API documents it.
    let request = captured(&seen);
    assert_eq!(request.method, "GET");
    assert_eq!(request.path, "/user");
    assert_eq!(request.header("authorization"), Some("Bearer test-token"));
    assert_eq!(
        request.header("accept"),
        Some("application/vnd.github+json")
    );
    assert!(
        request.header("user-agent").is_some(),
        "GitHub refuses a request without a User-Agent: {:?}",
        request.headers
    );
}

#[tokio::test]
async fn a_user_without_a_display_name_still_decodes() {
    let (base, _seen) = spawn_stub(200, r#"{"login":"octo-mock","name":null}"#).await;
    let user = client_for(&base)
        .current_user()
        .await
        .expect("a user without a name");
    assert_eq!(user.login, "octo-mock");
    assert_eq!(user.name, None);
}

#[tokio::test]
async fn a_rejected_token_is_auth_required_and_not_prose() {
    for status in [401, 403] {
        let (base, _seen) = spawn_stub(status, r#"{"message":"Bad credentials"}"#).await;
        let error = client_for(&base)
            .current_user()
            .await
            .expect_err("a rejected token");

        assert!(
            matches!(error, GitError::AuthRequired),
            "HTTP {status} has to be the variant a caller can match on: {error:?}"
        );
        assert_eq!(error.to_string(), "Authentication required");
        assert!(
            !error.to_string().contains(TOKEN),
            "an error an owner may see must not carry the token"
        );
    }
}

#[tokio::test]
async fn a_body_that_is_not_a_user_is_a_decode_error() {
    let (base, _seen) = spawn_stub(200, "<html>not GitHub</html>").await;
    let error = client_for(&base)
        .current_user()
        .await
        .expect_err("a body that is not a user");

    assert!(
        matches!(error, GitError::GitHub(_)),
        "an unusable body is the GitHub error variant: {error:?}"
    );
    assert!(!error.to_string().contains(TOKEN));

    // A JSON body that is a user but not this one's shape still has to be readable: GitHub
    // always sends `login`, and a body without it is what a proxy or a captive portal answers.
    let (base, _seen) = spawn_stub(200, r#"{"login":""}"#).await;
    let user = client_for(&base)
        .current_user()
        .await
        .expect("an empty login still decodes");
    assert_eq!(user.login, "");

    // And a non-2xx that is not about auth keeps the message shape the rest of the crate uses.
    let (base, _seen) = spawn_stub(500, r#"{"message":"Server Error"}"#).await;
    let error = client_for(&base)
        .current_user()
        .await
        .expect_err("a server error");
    let message = error.to_string();
    assert!(
        message.starts_with("GitHub error: GitHub returned HTTP 500: "),
        "{message}"
    );
}

/// The user payload, as JSON, so a caller that stores it does not hand-roll the mapping.
#[test]
fn a_user_serialises_to_the_json_github_sent() {
    let user = warsha_git::GithubUser {
        login: "octo-mock".to_string(),
        name: Some("Octo Mock".to_string()),
    };
    let value: Value = serde_json::to_value(&user).expect("a user serialises");
    assert_eq!(value["login"], "octo-mock");
    assert_eq!(value["name"], "Octo Mock");
}
