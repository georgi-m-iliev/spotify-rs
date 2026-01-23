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

    // Step 1: Get credentials and start librespot
    let auth_result = auth::perform_oauth_flow().await?;

    let audio_backend = Arc::new(AudioBackend::new(auth_result.clone()).await?);

    // Get player event channel for real-time playback updates
    let player_event_channel = audio_backend.get_player_event_channel().await
        .expect("Failed to get player event channel");

    // Step 2: Authenticate with rspotify for API control
    let rspotify_client = setup_rspotify(auth_result.rspotify_token).await?;

    match rspotify_client.me().await {
        Ok(user) => println!("✓ Rspotify authorized as: {}", user.id.to_string()),
        Err(e) => {
            eprintln!("❌ Rspotify authentication failed: {}", e);
            return Err(anyhow::anyhow!("Rspotify init failed"));
        }
    }

    // Create the SpotifyClient with our device name for targeting
    let device_name = AudioBackend::get_device_name().to_string();
    let spotify_client = SpotifyClient::new(rspotify_client, Some(device_name.clone()));

    // Initialize model
    let mut app_model = AppModel::new();
    app_model.set_spotify_client(spotify_client.clone());

    println!("\nStarting TUI... Press 'q' to quit.\n");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Wrap model in Arc<Mutex> for shared access
    let model = Arc::new(Mutex::new(app_model));

    // Set device name in playback settings
    model.lock().await.update_device_name(device_name).await;

    let controller = AppController::new(model.clone(), audio_backend.clone());

    // Start listening to librespot player events for real-time updates
    controller.start_player_event_listener(player_event_channel);

    // Initial refresh from Spotify API to get current track info
    controller.refresh_playback().await;

    // Load user's playlists
    controller.load_user_playlists().await;

    // Set initial volume to 70% via Spotify API
    // Wait a bit for device to be fully registered
    tokio::time::sleep(Duration::from_millis(500)).await;
    let model_guard = model.lock().await;
    if let Some(ref spotify) = model_guard.spotify {
        if spotify.set_volume(70).await.is_ok() {
            model_guard.set_volume(70).await;
        }
    }
    drop(model_guard);

    // Run the app
    let res = run_app(&mut terminal, model.clone(), controller).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("Error: {:?}", err);
    }

    // Keep audio backend alive until we exit
    drop(audio_backend);

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
