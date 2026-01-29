use std::fs;
use chrono::Utc;
use anyhow::Result;
use std::collections::HashSet;
use tracing;

use rspotify::Token;
use librespot::core::{authentication::Credentials, cache::Cache};

const SPOTIFY_CLIENT_ID: &str = "492e1e45ea814fa3ac555fe1576aaf5b";
const SPOTIFY_REDIRECT_URI: &str = "http://127.0.0.1:8898/login";
pub const SCOPES: &str =
    "streaming user-read-playback-state user-modify-playback-state user-read-currently-playing playlist-read-private playlist-read-collaborative playlist-modify-private playlist-modify-public user-read-playback-position user-top-read user-read-recently-played user-library-modify user-library-read";

const RESPONSE: &str = r#"
<!doctype html>
<html>
<head><title>Success</title></head>
<body><h1>Authentication Successful!</h1><script>window.close();</script></body>
</html>
"#;
const CACHE: &str = ".cache";
const CACHE_FILES: &str = ".cache/files";
const REFRESH_TOKEN_FILE: &str = ".cache/refresh_token";

#[derive(Clone)]
pub struct AuthResult {
    pub librespot_credentials: Credentials,
    pub rspotify_token: Token,
    pub cache: Cache,
}

async fn perform_browser_auth() -> Result<(Credentials, String)> {
    tracing::info!("Starting browser-based OAuth flow");
    let client = librespot_oauth::OAuthClientBuilder::new(
        SPOTIFY_CLIENT_ID,
        SPOTIFY_REDIRECT_URI,
        SCOPES.split_whitespace().collect(),
    )
    .open_in_browser()
    .with_custom_message(RESPONSE)
    .build()
    .expect("Failed to build OAuth client");

    let token = client
        .get_access_token_async()
        .await
        .expect("Failed to get token");

    let refresh_token = &token.refresh_token;

    let _ = fs::write(REFRESH_TOKEN_FILE, refresh_token);
    tracing::debug!("Saved refresh token to disk");

    let credentials = Credentials::with_access_token(token.access_token.clone());
    tracing::info!("Browser authentication completed successfully");
    Ok((credentials, token.access_token))
}

pub async fn perform_oauth_flow() -> Result<AuthResult> {
    let cache = Cache::new(Some(CACHE), Some(CACHE), Some(CACHE_FILES), None)?;

    let stored_refresh_token = fs::read_to_string(REFRESH_TOKEN_FILE).ok();

    let (credentials, access_token) =
        if let (Some(creds), Some(refresh_token)) = (cache.credentials(), stored_refresh_token) {
            tracing::info!("Found cached Librespot credentials and refresh token");

            let oauth_client = librespot_oauth::OAuthClientBuilder::new(
                SPOTIFY_CLIENT_ID,
                SPOTIFY_REDIRECT_URI,
                SCOPES.split_whitespace().collect(),
            )
            .build()?;

            match oauth_client.refresh_token_async(&refresh_token).await {
                Ok(new_token) => {
                    let rt = &new_token.refresh_token;
                    let _ = fs::write(REFRESH_TOKEN_FILE, rt);
                    tracing::debug!("Token refreshed successfully");

                    (creds, new_token.access_token)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Cached refresh token failed, re-authenticating");
                    perform_browser_auth().await?
                }
            }
        } else {
            tracing::info!("No cached credentials found, starting browser authentication");
            perform_browser_auth().await?
        };

    Ok(AuthResult {
        librespot_credentials: credentials,
        rspotify_token: Token {
            access_token,
            expires_in: chrono::Duration::seconds(3600),
            expires_at: Some(Utc::now() + chrono::Duration::seconds(3600)),
            scopes: SCOPES
                .split_whitespace()
                .map(|s| s.to_string())
                .collect::<HashSet<String>>(),
            refresh_token: None,
        },
        cache,
    })
}
