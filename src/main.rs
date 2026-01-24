mod audio;
mod auth;
mod controller;
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
    println!("=== Spotify-RS Client ===\n");

    // Step 1: Get credentials
    let auth_result = auth::perform_oauth_flow().await?;

    // Step 2: Authenticate with rspotify for API control (fast)
    let rspotify_client = setup_rspotify(auth_result.rspotify_token.clone()).await?;

    match rspotify_client.me().await {
        Ok(user) => println!("✓ Rspotify authorized as: {}", user.id.to_string()),
        Err(e) => {
            eprintln!("❌ Rspotify authentication failed: {}", e);
            return Err(anyhow::anyhow!("Rspotify init failed"));
        }
    }

    // Create the SpotifyClient with our local device name for reference
    let local_device_name = AudioBackend::get_device_name().to_string();
    let spotify_client = SpotifyClient::new(rspotify_client, Some(local_device_name.clone()));

    // Initialize liked songs cache from disk
    let cache_loaded = spotify_client.init_liked_songs_cache().await.is_ok();

    // If cache wasn't loaded from disk, refresh synchronously (first run)
    // Otherwise refresh in background
    if !cache_loaded || !std::path::Path::new(".cache/liked_songs.json").exists() {
        println!("Loading liked songs...");
        if let Err(e) = spotify_client.refresh_liked_songs_cache().await {
            eprintln!("Warning: Could not load liked songs: {}", e);
        }
    } else {
        // Refresh liked songs cache in background (async API call)
        let spotify_for_cache = spotify_client.clone();
        tokio::spawn(async move {
            let _ = spotify_for_cache.refresh_liked_songs_cache().await;
        });
    }

    // Initialize model
    let mut app_model = AppModel::new();
    app_model.set_spotify_client(spotify_client.clone());

    println!("\nStarting TUI...\n");

    // Setup terminal FIRST - show UI immediately
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Wrap model in Arc<Mutex> for shared access
    let model = Arc::new(Mutex::new(app_model));

    // Set initial device name
    model.lock().await.update_device_name(local_device_name).await;

    // Create a placeholder audio backend Arc that will be populated
    let audio_backend: Arc<Mutex<Option<AudioBackend>>> = Arc::new(Mutex::new(None));

    // Clone for background initialization
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

    // Load user's playlists (fast API call)
    controller.load_user_playlists().await;

    // Check current playback state in background (don't block UI)
    let controller_for_init = controller.clone();
    tokio::spawn(async move {
        controller_for_init.initialize_playback().await;
    });

    // Run the app
    let res = run_app(&mut terminal, model.clone(), controller).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("Error: {:?}", err);
    }

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

    println!("✓ Rspotify Initialized");

    *spotify.token.lock().await.unwrap() = Some(access_token);
    println!("✓ Rspotify Token Set");
    Ok(spotify)
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    model: Arc<Mutex<AppModel>>,
    controller: AppController,
) -> io::Result<()> {
    loop {
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
