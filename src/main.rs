mod audio;
mod auth;
mod controller;
mod logging;
mod model;
mod view;

use std::io;
use std::sync::Arc;
use anyhow::Result;
use std::time::Duration;
use tokio::sync::Mutex;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use rspotify::{clients::OAuthClient, AuthCodeSpotify, Config, Token};

use view::AppView;
use audio::AudioBackend;
use controller::AppController;
use model::{AppModel, SpotifyClient};

#[tokio::main]
async fn main() -> Result<()> {
    if let Err(e) = logging::init_logging() {
        eprintln!("Warning: Failed to initialize logging: {}", e);
    }

    tracing::info!("=== Spotify-RS Client Starting ===");

    // Step 1: Get credentials
    let auth_result = auth::perform_oauth_flow().await?;

    // Step 2: Authenticate with rspotify
    let rspotify_client = setup_rspotify(auth_result.rspotify_token.clone()).await?;

    match rspotify_client.me().await {
        Ok(user) => tracing::info!(user_id = %user.id, "rspotify authorized successfully"),
        Err(e) => {
            tracing::error!(error = %e, "rspotify authentication failed");
            return Err(anyhow::anyhow!("rspotify init failed"));
        }
    }

    // Create the SpotifyClient with our local device name
    let local_device_name = AudioBackend::get_device_name().to_string();
    let token_expires_at = auth_result.rspotify_token.expires_at;
    let spotify_client = SpotifyClient::new(
        rspotify_client,
        Some(local_device_name.clone()),
        auth_result.refresh_token.clone(),
        token_expires_at,
    );

    // Initialize liked songs cache from disk
    let cache_loaded = spotify_client.init_liked_songs_cache().await.is_ok();

    // If cache wasn't loaded from disk, refresh synchronously (first run)
    // Otherwise refresh in background
    if !cache_loaded || !std::path::Path::new(".cache/liked_songs.json").exists() {
        tracing::info!("Loading liked songs from API (first run or cache miss)...");
        if let Err(e) = spotify_client.refresh_liked_songs_cache().await {
            tracing::warn!(error = %e, "Could not load liked songs");
        }
    } else {
        tracing::debug!("Liked songs cache found, refreshing in background");
        // Refresh liked songs cache in background (async API call)
        let spotify_for_cache = spotify_client.clone();
        tokio::spawn(async move {
            let _ = spotify_for_cache.refresh_liked_songs_cache().await;
        });
    }

    let mut app_model = AppModel::new();
    app_model.set_spotify_client(spotify_client.clone());

    tracing::info!("Starting TUI...");

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let model = Arc::new(Mutex::new(app_model));

    // Set initial device name
    model.lock().await.update_device_name(local_device_name).await;

    let audio_backend: Arc<Mutex<Option<AudioBackend>>> = Arc::new(Mutex::new(None));

    let audio_backend_init = audio_backend.clone();
    let auth_for_backend = auth_result.clone();
    let model_for_init = model.clone();

    // Initialize audio backend in background
    tokio::spawn(async move {
        match AudioBackend::new(auth_for_backend).await {
            Ok(backend) => {
                // Get event channel before moving backend
                let event_channel = backend.get_player_event_channel().await;
                
                // Store the backend
                *audio_backend_init.lock().await = Some(backend);
                
                // Note: We can't start the event listener here because we don't have access to controller
                // The controller will check and start it when needed
                if event_channel.is_some() {
                    // Backend initialized successfully
                }
            }
            Err(e) => {
                let model = model_for_init.lock().await;
                model.set_error(format!("Audio init failed: {}", e)).await;
            }
        }
    });

    let controller = AppController::new(model.clone(), audio_backend.clone());

    controller.load_user_playlists().await;

    let controller_for_init = controller.clone();
    tokio::spawn(async move {
        controller_for_init.initialize_playback().await;
    });

    let res = run_app(&mut terminal, model.clone(), controller).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        tracing::error!(error = ?err, "Application error");
    }

    tracing::info!("Spotify-RS Client shutting down");
    Ok(())
}

async fn setup_rspotify(access_token: Token) -> Result<AuthCodeSpotify> {
    let spotify = AuthCodeSpotify::with_config(
        Default::default(),
        Default::default(),
        Config {
            token_cached: false,
            token_refreshing: false,
            ..Default::default()
        },
    );

    tracing::debug!("rspotify client initialized");

    *spotify.token.lock().await.unwrap() = Some(access_token);
    tracing::debug!("rspotify token set");
    Ok(spotify)
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    model: Arc<Mutex<AppModel>>,
    controller: AppController,
) -> io::Result<()> {
    // Track when we last checked the token
    let mut last_token_check = std::time::Instant::now();
    const TOKEN_CHECK_INTERVAL: Duration = Duration::from_secs(60); // Check every minute

    loop {
        // Periodically check and refresh token if needed
        if last_token_check.elapsed() >= TOKEN_CHECK_INTERVAL {
            last_token_check = std::time::Instant::now();

            let model_guard = model.lock().await;
            if let Some(spotify) = model_guard.get_spotify_client().await {
                drop(model_guard);
                tokio::spawn(async move {
                    match spotify.refresh_token_if_needed().await {
                        Ok(_) => {},
                        Err(e) => tracing::warn!("Token refresh check failed: {}", e),
                    }
                });
            } else {
                drop(model_guard);
            }
        }

        // Get current state
        let (playback, ui_state, content_state, should_quit) = {
            let model_guard = model.lock().await;

            // Auto-clear old errors (after 5 seconds)
            model_guard.auto_clear_old_errors().await;

            (
                model_guard.get_playback_info().await,
                model_guard.get_ui_state().await,
                model_guard.get_content_state().await,
                model_guard.should_quit().await,
            )
        };

        // Draw UI
        terminal.draw(|f| {
            AppView::render(f, &playback, &ui_state, &content_state);
        })?;

        // Handle input with shorter poll time for smoother UI updates
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                // Errors are now handled internally, no need to log
                let _ = controller.handle_key_event(key).await;
            }
        }

        if should_quit {
            break;
        }
    }

    Ok(())
}
