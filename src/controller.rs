use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::model::{AppModel, TrackInfo};

pub struct AppController {
    model: Arc<Mutex<AppModel>>,
}

impl AppController {
    pub fn new(model: Arc<Mutex<AppModel>>) -> Self {
        Self { model }
    }

    pub async fn handle_key_event(&self, key: KeyEvent) -> Result<()> {
        // Only handle key press events, not release or repeat
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                let model = self.model.lock().await;
                model.set_should_quit(true).await;
            }
            KeyCode::Char(' ') => {
                self.toggle_playback().await?;
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                self.next_track().await?;
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                self.previous_track().await?;
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.refresh_playback().await?;
            }
            _ => {}
        }
        Ok(())
    }

    async fn toggle_playback(&self) -> Result<()> {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            let is_playing = model.is_playing().await;

            if is_playing {
                spotify.pause().await?;
            } else {
                spotify.play().await?;
            }

            // Refresh state after toggling
            drop(model);
            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
            self.refresh_playback().await?;
        }

        Ok(())
    }

    async fn next_track(&self) -> Result<()> {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            spotify.next_track().await?;
        }

        // Refresh after action
        drop(model);
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        self.refresh_playback().await?;

        Ok(())
    }

    async fn previous_track(&self) -> Result<()> {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            spotify.previous_track().await?;
        }

        // Refresh after action
        drop(model);
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        self.refresh_playback().await?;

        Ok(())
    }

    pub async fn refresh_playback(&self) -> Result<()> {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            if let Some(playback) = spotify.get_current_playback().await? {
                let track = TrackInfo::from_playback(&playback);
                let is_playing = playback.is_playing;
                model.update_playback_state(track, is_playing).await;
            }
        }

        Ok(())
    }

    pub async fn start_playback_monitor(&self) {
        let model = self.model.clone();

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

                let model_guard = model.lock().await;
                if model_guard.should_quit().await {
                    break;
                }

                if let Some(spotify) = &model_guard.spotify {
                    if let Ok(Some(playback)) = spotify.get_current_playback().await {
                        let track = TrackInfo::from_playback(&playback);
                        let is_playing = playback.is_playing;
                        model_guard.update_playback_state(track, is_playing).await;
                    }
                }
            }
        });
    }
}
