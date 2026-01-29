use crate::auth::AuthResult;
use anyhow::Result;
use librespot::connect::{ConnectConfig, Spirc};
use librespot::core::config::SessionConfig;
use librespot::core::session::Session;
use librespot::playback::config::{AudioFormat, Bitrate, PlayerConfig};
use librespot::playback::mixer::MixerConfig;
use librespot::playback::player::{Player, PlayerEventChannel};
use librespot::playback::{audio_backend, mixer};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing;

const DEVICE_NAME: &str = "Spotify-RS";

/// Default volume as a percentage (0-100)
pub const DEFAULT_VOLUME_PERCENT: u8 = 70;

/// Convert percentage (0-100) to librespot volume (0-65535)
pub fn percent_to_librespot_volume(percent: u8) -> u16 {
    ((percent as f32 / 100.0) * 65535.0) as u16
}

pub struct AudioPlayer {
    pub player: Arc<Player>,
    #[allow(dead_code)]
    session: Session,
    spirc: Spirc,
    is_active: bool,
}

/// Wrapper for the audio backend
/// The backend is created on startup but NOT activated until needed
/// This allows fast startup while preserving existing Spotify playback state
pub struct AudioBackend {
    inner: Mutex<Option<AudioPlayer>>,
    auth: AuthResult,
}

impl AudioPlayer {
    /// Create a new audio player
    /// - silent: don't print initialization messages (for TUI mode)
    /// - activate: whether to activate the device immediately
    async fn new_internal(auth: AuthResult, silent: bool, activate: bool) -> Result<Self> {
        if !silent {
            tracing::info!("Initializing audio backend...");
        }
        tracing::debug!(device_name = DEVICE_NAME, "Creating audio player");

        // Create session configuration
        let session_config = SessionConfig {
            device_id: Self::get_device_id(),
            ..Default::default()
        };

        let player_config = PlayerConfig {
            bitrate: Bitrate::Bitrate320,
            // Enable position updates every 500ms for smooth progress tracking
            position_update_interval: Some(Duration::from_millis(500)),
            ..Default::default()
        };
        let audio_format = AudioFormat::default();

        // Initial volume must match the default in model::PlaybackSettings
        let initial_volume = percent_to_librespot_volume(DEFAULT_VOLUME_PERCENT);
        let connect_config = ConnectConfig {
            name: DEVICE_NAME.to_string(),
            initial_volume,
            ..Default::default()
        };
        let mixer_config = MixerConfig::default();
        let sink_builder = audio_backend::find(None).unwrap();
        let mixer_builder = mixer::find(None).unwrap();

        // Clone cache so we can still use auth later
        let session = Session::new(session_config, Some(auth.cache.clone()));

        let mixer = mixer_builder(mixer_config)?;

        let player = Player::new(
            player_config,
            session.clone(),
            mixer.get_soft_volume(),
            move || sink_builder(None, audio_format),
        );

        // Clone credentials so we can still use auth
        let credentials = auth.librespot_credentials.clone();
        let (spirc, spirc_task) = Spirc::new(
            connect_config,
            session.clone(),
            credentials,
            player.clone(),
            mixer,
        )
        .await?;

        // Only activate if requested
        let is_active = if activate {
            spirc.activate()?;
            tracing::debug!("Audio device activated");
            true
        } else {
            false
        };

        tokio::spawn(async move {
            let _spirc_task_res = spirc_task.await;
        });

        if !silent {
            tracing::info!(device_name = DEVICE_NAME, "Audio backend ready");
        }
        tracing::debug!(device_id = %Self::get_device_id(), is_active, "Audio player created");

        Ok(Self {
            player,
            session,
            spirc,
            is_active,
        })
    }

    /// Activate the device (make it available for Spotify Connect)
    pub fn activate(&mut self) -> Result<()> {
        if !self.is_active {
            self.spirc.activate()?;
            self.is_active = true;
            tracing::info!(device_name = DEVICE_NAME, "Audio device activated for Spotify Connect");
        }
        Ok(())
    }

    /// Get the player event channel for receiving playback state updates
    pub fn get_player_event_channel(&self) -> PlayerEventChannel {
        self.player.get_player_event_channel()
    }

    fn get_device_id() -> String {
        // Generate a consistent device ID based on machine
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        format!("{}-{}", AudioPlayer::get_device_name(), hostname)
    }

    pub fn get_device_name() -> &'static str {
        DEVICE_NAME
    }
}

impl AudioBackend {
    /// Create a new AudioBackend and initialize the player (but don't activate it)
    /// This creates the librespot session but doesn't make it the active device
    /// Call `activate()` later to make it available for playback
    pub async fn new(auth: AuthResult) -> Result<Self> {
        // Create the player but don't activate it yet (silent mode - TUI may be active)
        let player = AudioPlayer::new_internal(auth.clone(), true, false).await?;
        Ok(Self {
            inner: Mutex::new(Some(player)),
            auth,
        })
    }

    /// Get the player event channel
    pub async fn get_player_event_channel(&self) -> Option<PlayerEventChannel> {
        let guard = self.inner.lock().await;
        guard.as_ref().map(|p| p.get_player_event_channel())
    }

    pub fn get_device_name() -> &'static str {
        AudioPlayer::get_device_name()
    }

    /// Check if the audio backend is activated (available for Spotify Connect)
    pub async fn is_active(&self) -> bool {
        let guard = self.inner.lock().await;
        guard.as_ref().map(|p| p.is_active).unwrap_or(false)
    }

    /// Activate the local device (make it available for Spotify Connect)
    /// This should be called when the user wants to play on the local device
    pub async fn activate(&self) -> Result<()> {
        let mut guard = self.inner.lock().await;
        if let Some(player) = guard.as_mut() {
            player.activate()?;
        }
        Ok(())
    }

    /// Stop the local audio playback (used when switching to another device)
    pub async fn stop(&self) {
        let guard = self.inner.lock().await;
        if let Some(player) = guard.as_ref() {
            player.player.stop();
        }
    }

    /// Restart the audio backend (silently, for recovery)
    /// Returns the player event channel for listening to playback events
    pub async fn restart(&self) -> Result<PlayerEventChannel> {
        tracing::info!("Restarting audio backend for recovery");
        // Drop the old player
        {
            let mut guard = self.inner.lock().await;
            *guard = None;
        }

        // Small delay to ensure cleanup
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Create a new player and activate it (restart is for recovery, so activate)
        let new_player = AudioPlayer::new_internal(self.auth.clone(), true, true).await?;
        let event_channel = new_player.get_player_event_channel();

        {
            let mut guard = self.inner.lock().await;
            *guard = Some(new_player);
        }

        tracing::info!("Audio backend restarted successfully");
        Ok(event_channel)
    }

    /// Skip to the next track (used for auto-skip when a track is in the skip list)
    pub async fn skip_to_next(&self) -> Result<()> {
        let guard = self.inner.lock().await;
        if let Some(player) = guard.as_ref() {
            player.spirc.next()?;
            tracing::debug!("Skipped to next track via spirc");
        }
        Ok(())
    }
}
