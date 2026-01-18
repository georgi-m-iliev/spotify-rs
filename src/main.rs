mod audio;
mod auth;
mod controller;
mod model;
mod view;

use anyhow::Result;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use rspotify::{clients::OAuthClient, AuthCodeSpotify, Config, Token};
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use audio::AudioPlayer;
use controller::AppController;
use model::{AppModel, SpotifyClient};
use view::AppView;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== Spotify-RS Client ===\n");

    // Step 1: Get credentials and start librespot
    let auth_result = auth::perform_oauth_flow().await?;

    let player = AudioPlayer::new(auth_result.clone()).await?;

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
    let device_name = AudioPlayer::get_device_name().to_string();
    let spotify_client = SpotifyClient::new(rspotify_client, Some(device_name));

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
    let controller = AppController::new(model.clone());

    // Start background playback monitor
    controller.start_playback_monitor().await;

    // Initial refresh
    controller.refresh_playback().await?;

    // Run the app
    let res = run_app(&mut terminal, model.clone(), controller).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("Error: {:?}", err);
    }

    // Keep audio player alive until we exit
    drop(player);

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
        let (track, is_playing, should_quit) = {
            let model_guard = model.lock().await;
            (
                model_guard.get_track_info().await,
                model_guard.is_playing().await,
                model_guard.should_quit().await,
            )
        };

        // Draw UI
        terminal.draw(|f| {
            AppView::render(f, &track, is_playing);
        })?;

        // Handle input
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if let Err(e) = controller.handle_key_event(key).await {
                    // Log error but continue running
                    eprintln!("Error handling key event: {}", e);
                }
            }
        }

        if should_quit {
            break;
        }
    }

    Ok(())
}
