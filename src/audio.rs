 use crate::auth::AuthResult;
use anyhow::Result;
use librespot::connect::{ConnectConfig, Spirc};
use librespot::core::config::SessionConfig;
use librespot::core::session::Session;
use librespot::playback::config::{AudioFormat, Bitrate, PlayerConfig};
use librespot::playback::mixer::{MixerConfig, NoOpVolume};
use librespot::playback::player::Player;
use librespot::playback::{audio_backend, mixer};
use std::sync::Arc;

const DEVICE_NAME: &str = "Spotify-RS";

pub struct AudioPlayer {
    pub player: Arc<Player>,
    session: Session,
    spirc: Spirc,
}

impl AudioPlayer {
    pub async fn new(auth: AuthResult) -> Result<Self> {
        println!("Connecting to Spotify with librespot...");

        // Create session configuration
        let session_config = SessionConfig {
            device_id: Self::get_device_id(),
            ..Default::default()
        };

        let player_config = PlayerConfig {
            bitrate: Bitrate::Bitrate320,
            ..Default::default()
        };
        let audio_format = AudioFormat::default();
        let connect_config = ConnectConfig::default();
        let mixer_config = MixerConfig::default();
        let sink_builder = audio_backend::find(None).unwrap();
        let mixer_builder = mixer::find(None).unwrap();

        println!("... Connecting librespot");
        let session = Session::new(session_config, Some(auth.cache));

        let mixer = mixer_builder(mixer_config)?;

        let player = Player::new(
            player_config,
            session.clone(),
            Box::new(NoOpVolume),
            move || sink_builder(None, audio_format),
        );

        let (spirc, spirc_task) = Spirc::new(
            connect_config,
            session.clone(),
            auth.librespot_credentials,
            player.clone(),
            mixer,
        )
        .await?;

        spirc.activate()?;

        tokio::spawn(async move {
            let _spirc_task_res = spirc_task.await;
        });

        println!("✓ Audio player initialized!");
        println!("  Device name: {}", DEVICE_NAME);

        Ok(Self {
            player,
            session,
            spirc,
        })
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

    pub fn username(&self) -> String {
        self.session.username()
    }
}
