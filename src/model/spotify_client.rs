//! Spotify API client wrapper with all API methods

use std::sync::Arc;
use anyhow::Result;
use tokio::sync::RwLock;
use rspotify::{
    model::{CurrentPlaybackContext, PlayableItem, SearchType, Market, AlbumId, PlaylistId, ArtistId, TrackId, PlayContextId, PlayableId},
    prelude::*,
    AuthCodeSpotify,
};

use super::cache::LikedSongsCache;
use super::types::{DeviceInfo, PlaylistItem, RepeatState};
use super::content::{
    SearchTrack, SearchAlbum, SearchArtist, SearchPlaylist,
    SearchResults, AlbumDetail, PlaylistDetail, ArtistDetail,
};

/// Spotify API client with caching and token refresh support
#[derive(Clone)]
pub struct SpotifyClient {
    client: Arc<AuthCodeSpotify>,
    local_device_name: Option<String>,
    liked_songs_cache: LikedSongsCache,
    refresh_token: Arc<RwLock<String>>,
    token_expires_at: Arc<RwLock<Option<chrono::DateTime<chrono::Utc>>>>,
}

impl SpotifyClient {
    pub fn new(
        client: AuthCodeSpotify,
        local_device_name: Option<String>,
        refresh_token: String,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Self {
        Self {
            client: Arc::new(client),
            local_device_name,
            liked_songs_cache: LikedSongsCache::new(),
            refresh_token: Arc::new(RwLock::new(refresh_token)),
            token_expires_at: Arc::new(RwLock::new(expires_at)),
        }
    }

    pub async fn token_needs_refresh(&self) -> bool {
        let expires_at = self.token_expires_at.read().await;
        if let Some(exp) = *expires_at {
            let now = chrono::Utc::now();
            let remaining = exp - now;
            // Refresh if less than 5 minutes remaining
            remaining.num_seconds() < 300
        } else {
            false
        }
    }

    pub async fn refresh_token_if_needed(&self) -> Result<bool> {
        if !self.token_needs_refresh().await {
            return Ok(false);
        }

        let refresh_token = self.refresh_token.read().await.clone();

        tracing::info!("Token expiring soon, refreshing...");

        match crate::auth::refresh_access_token(&refresh_token).await {
            Ok((new_access_token, new_refresh_token, new_expires_at)) => {
                // Update the client's token
                use rspotify::Token;
                use std::collections::HashSet;

                let new_token = Token {
                    access_token: new_access_token,
                    expires_in: chrono::Duration::seconds(3600),
                    expires_at: Some(new_expires_at),
                    scopes: crate::auth::SCOPES
                        .split_whitespace()
                        .map(|s| s.to_string())
                        .collect::<HashSet<String>>(),
                    refresh_token: None,
                };

                *self.client.token.lock().await.unwrap() = Some(new_token);

                // Update stored refresh token and expiry
                *self.refresh_token.write().await = new_refresh_token;
                *self.token_expires_at.write().await = Some(new_expires_at);

                tracing::info!("Token refreshed successfully");
                Ok(true)
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to refresh token");
                Err(e)
            }
        }
    }

    pub async fn init_liked_songs_cache(&self) -> Result<()> {
        // First try to load from disk
        if let Err(_e) = self.liked_songs_cache.load_from_disk().await {
            // Silently ignore - cache file may not exist yet
        }
        Ok(())
    }

    pub async fn refresh_liked_songs_cache(&self) -> Result<()> {
        use futures::TryStreamExt;
        use futures::StreamExt;

        tracing::debug!("Refreshing liked songs cache from API");

        let tracks_stream = self.client.current_user_saved_tracks(None);
        let saved_tracks: Vec<_> = tracks_stream
            .take(1000) // Reasonable limit
            .try_collect()
            .await?;

        let track_ids: Vec<String> = saved_tracks
            .into_iter()
            .filter_map(|saved| saved.track.id.map(|id| id.id().to_string()))
            .collect();

        tracing::info!(count = track_ids.len(), "Liked songs cache refreshed");

        // Update cache
        self.liked_songs_cache.update(track_ids).await;

        // Save to disk
        let _ = self.liked_songs_cache.save_to_disk().await;

        Ok(())
    }

    pub async fn mark_tracks_liked(&self, tracks: &mut [SearchTrack]) {
        for track in tracks.iter_mut() {
            track.liked = self.liked_songs_cache.is_liked(&track.id).await;
        }
    }

    pub async fn add_to_liked_songs(&self, track_id: &str) -> Result<()> {
        if track_id.is_empty() {
            return Err(anyhow::anyhow!("Track ID is empty"));
        }

        tracing::debug!(track_id, "Adding track to liked songs");
        let id = TrackId::from_id(track_id)?;
        self.client.current_user_saved_tracks_add([id]).await?;

        // Update the cache
        self.liked_songs_cache.add(track_id.to_string()).await;
        let _ = self.liked_songs_cache.save_to_disk().await;

        tracing::info!(track_id, "Added track to liked songs");
        Ok(())
    }

    pub async fn remove_from_liked_songs(&self, track_id: &str) -> Result<()> {
        if track_id.is_empty() {
            return Err(anyhow::anyhow!("Track ID is empty"));
        }

        tracing::debug!(track_id, "Removing track from liked songs");
        let id = TrackId::from_id(track_id)?;
        self.client.current_user_saved_tracks_delete([id]).await?;

        // Update the cache
        self.liked_songs_cache.remove(track_id).await;
        let _ = self.liked_songs_cache.save_to_disk().await;

        tracing::info!(track_id, "Removed track from liked songs");
        Ok(())
    }

    pub async fn toggle_liked_song(&self, track_id: &str) -> Result<bool> {
        let is_liked = self.liked_songs_cache.is_liked(track_id).await;

        if is_liked {
            self.remove_from_liked_songs(track_id).await?;
            Ok(false)
        } else {
            self.add_to_liked_songs(track_id).await?;
            Ok(true)
        }
    }
    
    pub async fn get_current_playback(&self) -> Result<Option<CurrentPlaybackContext>> {
        tracing::trace!("Fetching current playback state");
        let result = self.client.current_playback(None, None::<Vec<_>>).await?;
        if let Some(ref playback) = result {
            tracing::trace!(
                is_playing = playback.is_playing,
                device = ?playback.device.name,
                "Got playback state"
            );
        }
        Ok(result)
    }

    async fn get_device_id(&self) -> Option<String> {
        if let Ok(devices) = self.client.device().await {
            // First, try to find the active device
            let active_device = devices.iter().find(|d| d.is_active);
            if let Some(device) = active_device {
                tracing::debug!(device_name = %device.name, device_id = ?device.id, "Found active device");
                return device.id.clone();
            }

            // No active device - try to find our local device as fallback
            if let Some(local_name) = &self.local_device_name {
                let local_device = devices.iter().find(|d| &d.name == local_name);
                if let Some(device) = local_device {
                    tracing::debug!(device_name = %device.name, device_id = ?device.id, "No active device, using local device as fallback");
                    return device.id.clone();
                }
            }

            tracing::debug!(available_devices = devices.len(), "No active device found and local device not in list");
            None
        } else {
            tracing::debug!("Failed to get devices list");
            None
        }
    }

    pub async fn play(&self) -> Result<()> {
        let device_id = self.get_device_id().await;
        tracing::debug!(device_id = ?device_id, "API: resume_playback");
        self.client
            .resume_playback(device_id.as_deref(), None)
            .await?;
        Ok(())
    }

    pub async fn pause(&self) -> Result<()> {
        let device_id = self.get_device_id().await;
        tracing::debug!(device_id = ?device_id, "API: pause_playback");
        self.client.pause_playback(device_id.as_deref()).await?;
        Ok(())
    }

    pub async fn next_track(&self) -> Result<()> {
        let device_id = self.get_device_id().await;
        tracing::debug!(device_id = ?device_id, "API: next_track");
        self.client.next_track(device_id.as_deref()).await?;
        Ok(())
    }

    pub async fn previous_track(&self) -> Result<()> {
        let device_id = self.get_device_id().await;
        tracing::debug!(device_id = ?device_id, "API: previous_track");
        self.client.previous_track(device_id.as_deref()).await?;
        Ok(())
    }

    pub async fn set_shuffle(&self, state: bool) -> Result<()> {
        let device_id = self.get_device_id().await;
        tracing::debug!(state, device_id = ?device_id, "API: set_shuffle");
        self.client.shuffle(state, device_id.as_deref()).await?;
        Ok(())
    }

    pub async fn set_repeat(&self, state: RepeatState) -> Result<()> {
        let device_id = self.get_device_id().await;
        tracing::debug!(state = ?state, device_id = ?device_id, "API: set_repeat");
        let repeat_state = match state {
            RepeatState::Off => rspotify::model::RepeatState::Off,
            RepeatState::All => rspotify::model::RepeatState::Context,
            RepeatState::One => rspotify::model::RepeatState::Track,
        };
        self.client.repeat(repeat_state, device_id.as_deref()).await?;
        Ok(())
    }

    pub async fn set_volume(&self, volume: u8) -> Result<()> {
        let device_id = self.get_device_id().await;
        tracing::debug!(volume, device_id = ?device_id, "API: set_volume");
        self.client.volume(volume, device_id.as_deref()).await?;
        Ok(())
    }
    
    pub async fn get_available_devices(&self) -> Result<Vec<DeviceInfo>> {
        tracing::debug!("API: get_available_devices");
        let devices = self.client.device().await?;
        let device_infos: Vec<DeviceInfo> = devices
            .into_iter()
            .map(|d| DeviceInfo {
                id: d.id.unwrap_or_default(),
                name: d.name,
                is_active: d.is_active,
            })
            .collect();
        tracing::debug!(count = device_infos.len(), "Found devices");
        Ok(device_infos)
    }

    pub async fn has_active_device(&self) -> bool {
        if let Ok(devices) = self.client.device().await {
            devices.iter().any(|d| d.is_active)
        } else {
            false
        }
    }

    pub async fn transfer_playback_to_device(&self, device_id: &str, start_playing: bool) -> Result<()> {
        tracing::debug!(device_id, start_playing, "API: transfer_playback");
        self.client
            .transfer_playback(device_id, Some(start_playing))
            .await?;
        Ok(())
    }

    pub async fn search(&self, query: &str, limit: u32) -> Result<SearchResults> {
        // Use None for market to let Spotify use the user's account country
        let market: Option<Market> = None;
        let mut results = SearchResults::default();

        // Search all types in parallel using futures::join!
        let (track_result, album_result, artist_result, playlist_result) = futures::join!(
            self.client.search(query, SearchType::Track, market, None, Some(limit), None),
            self.client.search(query, SearchType::Album, market, None, Some(limit), None),
            self.client.search(query, SearchType::Artist, market, None, Some(limit), None),
            self.client.search(query, SearchType::Playlist, market, None, Some(limit), None)
        );

        // Process track results
        if let Ok(rspotify::model::SearchResult::Tracks(page)) = track_result {
            for track in page.items {
                let track_id = track.id.as_ref().map(|id| id.id().to_string()).unwrap_or_default();
                let all_artists: Vec<String> = track.artists.iter().map(|a| a.name.clone()).collect();
                results.tracks.push(SearchTrack {
                    uri: format!("spotify:track:{}", track_id),
                    id: track_id,
                    name: track.name,
                    artist: track.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
                    artists: all_artists,
                    album: track.album.name,
                    duration_ms: track.duration.num_milliseconds() as u32,
                    liked: false, // Set by mark_tracks_liked() in controller
                });
            }
        }

        // Process album results
        if let Ok(rspotify::model::SearchResult::Albums(page)) = album_result {
            for album in page.items {
                results.albums.push(SearchAlbum {
                    id: album.id.as_ref().map(|id| id.id().to_string()).unwrap_or_default(),
                    name: album.name,
                    artist: album.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
                    year: album.release_date.unwrap_or_default().chars().take(4).collect(),
                });
            }
        }

        // Process artist results
        if let Ok(rspotify::model::SearchResult::Artists(page)) = artist_result {
            for artist in page.items {
                results.artists.push(SearchArtist {
                    id: artist.id.id().to_string(),
                    name: artist.name,
                    genres: artist.genres,
                });
            }
        }

        // Process playlist results
        if let Ok(rspotify::model::SearchResult::Playlists(page)) = playlist_result {
            for playlist in page.items {
                let playlist_id = playlist.id.id().to_string();
                results.playlists.push(SearchPlaylist {
                    id: playlist_id.clone(),
                    name: playlist.name,
                    owner: playlist.owner.display_name.unwrap_or_else(|| playlist.owner.id.id().to_string()),
                    total_tracks: playlist.tracks.total,
                });
            }
        }

        // Determine which category best matches the query
        results.determine_best_match(query);

        Ok(results)
    }

    pub async fn get_album(&self, album_id: &str) -> Result<AlbumDetail> {
        let id = AlbumId::from_id(album_id)?;
        let album = self.client.album(id.clone(), None).await?;

        let mut tracks = Vec::new();

        // Album tracks are included in the full album response
        for track in album.tracks.items.iter() {
            let track_id = track.id.as_ref().map(|id| id.id().to_string()).unwrap_or_default();
            let all_artists: Vec<String> = track.artists.iter().map(|a| a.name.clone()).collect();
            tracks.push(SearchTrack {
                uri: format!("spotify:track:{}", track_id),
                id: track_id,
                name: track.name.clone(),
                artist: track.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
                artists: all_artists,
                album: album.name.clone(),
                duration_ms: track.duration.num_milliseconds() as u32,
                liked: false, // Set by mark_tracks_liked() in controller
            });
        }

        Ok(AlbumDetail {
            id: album_id.to_string(),
            name: album.name,
            artist: album.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
            year: album.release_date.chars().take(4).collect(),
            tracks,
        })
    }

    pub const PLAYLIST_PAGE_SIZE: usize = 100;

    pub async fn get_playlist(&self, playlist_id: &str) -> Result<PlaylistDetail> {
        self.get_playlist_with_offset(playlist_id, 0).await
    }

    pub async fn get_playlist_with_offset(&self, playlist_id: &str, offset: usize) -> Result<PlaylistDetail> {
        use futures::TryStreamExt;
        use futures::StreamExt;

        let id = PlaylistId::from_id(playlist_id)?;

        // First get playlist metadata
        let playlist = self.client.playlist(id.clone(), None, None).await?;
        let total_tracks = playlist.tracks.total;

        // Fetch tracks with pagination using playlist_items stream
        let items_stream = self.client.playlist_items(id.clone(), None, None);
        let items: Vec<_> = items_stream
            .skip(offset)
            .take(Self::PLAYLIST_PAGE_SIZE)
            .try_collect()
            .await?;

        let mut tracks = Vec::new();
        for item in items.iter() {
            if let Some(PlayableItem::Track(track)) = &item.track {
                let track_id = track.id.as_ref().map(|id| id.id().to_string()).unwrap_or_default();
                let all_artists: Vec<String> = track.artists.iter().map(|a| a.name.clone()).collect();
                tracks.push(SearchTrack {
                    uri: format!("spotify:track:{}", track_id),
                    id: track_id,
                    name: track.name.clone(),
                    artist: track.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
                    artists: all_artists,
                    album: track.album.name.clone(),
                    duration_ms: track.duration.num_milliseconds() as u32,
                    liked: false, // Set by mark_tracks_liked() in controller
                });
            }
        }

        let loaded_count = offset + tracks.len();
        let has_more = loaded_count < total_tracks as usize;

        Ok(PlaylistDetail {
            id: playlist_id.to_string(),
            uri: format!("spotify:playlist:{}", playlist_id),
            name: playlist.name,
            owner: playlist.owner.display_name.clone().unwrap_or_else(|| playlist.owner.id.to_string()),
            tracks,
            total_tracks,
            has_more,
            loading_more: false,
        })
    }

    pub async fn get_more_playlist_tracks(&self, playlist_id: &str, offset: usize) -> Result<(Vec<SearchTrack>, u32, bool)> {
        use futures::TryStreamExt;
        use futures::StreamExt;

        let id = PlaylistId::from_id(playlist_id)?;

        // Get total count from playlist metadata
        let playlist = self.client.playlist(id.clone(), None, None).await?;
        let total_tracks = playlist.tracks.total;

        // Fetch next batch of tracks using stream
        let items_stream = self.client.playlist_items(id.clone(), None, None);
        let items: Vec<_> = items_stream
            .skip(offset)
            .take(Self::PLAYLIST_PAGE_SIZE)
            .try_collect()
            .await?;

        let mut tracks = Vec::new();
        for item in items.iter() {
            if let Some(PlayableItem::Track(track)) = &item.track {
                let track_id = track.id.as_ref().map(|id| id.id().to_string()).unwrap_or_default();
                let all_artists: Vec<String> = track.artists.iter().map(|a| a.name.clone()).collect();
                tracks.push(SearchTrack {
                    uri: format!("spotify:track:{}", track_id),
                    id: track_id,
                    name: track.name.clone(),
                    artist: track.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
                    artists: all_artists,
                    album: track.album.name.clone(),
                    duration_ms: track.duration.num_milliseconds() as u32,
                    liked: false, // Set by mark_tracks_liked() in controller
                });
            }
        }

        // Check if there are more tracks to load
        let loaded_count = offset + tracks.len();
        let has_more = loaded_count < total_tracks as usize;

        Ok((tracks, total_tracks, has_more))
    }

    pub async fn get_artist(&self, artist_id: &str) -> Result<ArtistDetail> {
        use futures::TryStreamExt;

        let id = ArtistId::from_id(artist_id)?;
        let artist = self.client.artist(id.clone()).await?;

        // Get top tracks - use FromToken to use user's account country
        let market = Market::FromToken;
        let top_tracks_result = self.client.artist_top_tracks(id.clone(), Some(market)).await?;

        let top_tracks: Vec<SearchTrack> = top_tracks_result
            .into_iter()
            .map(|track| {
                let track_id = track.id.as_ref().map(|id| id.id().to_string()).unwrap_or_default();
                let all_artists: Vec<String> = track.artists.iter().map(|a| a.name.clone()).collect();
                SearchTrack {
                    uri: format!("spotify:track:{}", track_id),
                    id: track_id,
                    name: track.name,
                    artist: track.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
                    artists: all_artists,
                    album: track.album.name,
                    duration_ms: track.duration.num_milliseconds() as u32,
                    liked: false, // Set by mark_tracks_liked() in controller
                }
            })
            .collect();

        // Get albums - artist_albums returns a stream
        let album_stream = self.client.artist_albums(id, None, None);
        let album_pages: Vec<_> = album_stream.try_collect().await?;

        let albums: Vec<SearchAlbum> = album_pages
            .into_iter()
            .map(|album| SearchAlbum {
                id: album.id.as_ref().map(|i| i.id().to_string()).unwrap_or_default(),
                name: album.name,
                artist: album.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
                year: album.release_date.unwrap_or_default().chars().take(4).collect(),
            })
            .collect();

        Ok(ArtistDetail {
            name: artist.name,
            genres: artist.genres,
            top_tracks,
            albums,
        })
    }

    pub async fn play_track(&self, uri: &str) -> Result<()> {
        let device_id = self.get_device_id().await;
        tracing::debug!(uri, device_id = ?device_id, "API: play_track");

        // Extract track ID from URI (format: spotify:track:ID)
        let track_id = uri.split(':').last().unwrap_or(uri);

        self.client
            .start_uris_playback(
                [PlayableId::Track(TrackId::from_id(track_id)?)],
                device_id.as_deref(),
                None,
                None,
            )
            .await?;
        Ok(())
    }

    pub async fn play_context_from_track_uri(&self, context_uri: &str, track_uri: &str) -> Result<()> {
        let device_id = self.get_device_id().await;

        // Parse the URI to determine type
        let play_context = if context_uri.contains(":album:") {
            let id = context_uri.split(':').last().unwrap_or("");
            PlayContextId::Album(AlbumId::from_id(id)?)
        } else if context_uri.contains(":playlist:") {
            let id = context_uri.split(':').last().unwrap_or("");
            PlayContextId::Playlist(PlaylistId::from_id(id)?)
        } else if context_uri.contains(":artist:") {
            let id = context_uri.split(':').last().unwrap_or("");
            PlayContextId::Artist(ArtistId::from_id(id)?)
        } else {
            return Err(anyhow::anyhow!("Unknown context type: {}", context_uri));
        };

        // Use Offset::Uri to start from a specific track
        let offset = rspotify::model::Offset::Uri(track_uri.to_string());

        self.client
            .start_context_playback(play_context, device_id.as_deref(), Some(offset), None)
            .await?;
        Ok(())
    }

    pub async fn get_queue(&self) -> Result<(Option<SearchTrack>, Vec<SearchTrack>)> {
        let queue_result = self.client.current_user_queue().await?;

        // Convert currently playing track
        let currently_playing = if let Some(item) = queue_result.currently_playing {
            match item {
                PlayableItem::Track(track) => {
                    let track_id = track.id.as_ref().map(|id| id.id().to_string()).unwrap_or_default();
                    if !track_id.is_empty() {
                        let all_artists: Vec<String> = track.artists.iter().map(|a| a.name.clone()).collect();
                        Some(SearchTrack {
                            id: track_id.clone(),
                            uri: format!("spotify:track:{}", track_id),
                            name: track.name.clone(),
                            artist: track.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
                            artists: all_artists,
                            album: track.album.name.clone(),
                            duration_ms: track.duration.num_milliseconds() as u32,
                            liked: self.liked_songs_cache.is_liked(&track_id).await,
                        })
                    } else {
                        None
                    }
                }
                _ => None,
            }
        } else {
            None
        };

        // Convert queue tracks
        let mut queue_tracks = Vec::new();
        for item in queue_result.queue {
            if let PlayableItem::Track(track) = item {
                let track_id = track.id.as_ref().map(|id| id.id().to_string()).unwrap_or_default();
                if !track_id.is_empty() {
                    let all_artists: Vec<String> = track.artists.iter().map(|a| a.name.clone()).collect();
                    queue_tracks.push(SearchTrack {
                        id: track_id.clone(),
                        uri: format!("spotify:track:{}", track_id),
                        name: track.name.clone(),
                        artist: track.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
                        artists: all_artists,
                        album: track.album.name.clone(),
                        duration_ms: track.duration.num_milliseconds() as u32,
                        liked: self.liked_songs_cache.is_liked(&track_id).await,
                    });
                }
            }
        }

        Ok((currently_playing, queue_tracks))
    }

    pub async fn add_to_queue(&self, track_uri: &str) -> Result<()> {
        let track_id = track_uri.split(':').last().unwrap_or(track_uri);
        let id = TrackId::from_id(track_id)?;
        let device_id = self.get_device_id().await;

        self.client.add_item_to_queue(PlayableId::Track(id), device_id.as_deref()).await?;

        tracing::info!(track_uri, "Added track to queue");
        Ok(())
    }

    pub async fn get_user_playlists(&self, limit: u32) -> Result<Vec<PlaylistItem>> {
        use futures::TryStreamExt;

        let playlist_stream = self.client.current_user_playlists();
        let all_playlists: Vec<_> = playlist_stream.try_collect().await?;

        let playlists: Vec<PlaylistItem> = all_playlists
            .into_iter()
            .take(limit as usize)
            .map(|playlist| {
                let id = playlist.id.id().to_string();
                PlaylistItem {
                    id: id.clone(),
                    name: playlist.name,
                }
            })
            .collect();

        Ok(playlists)
    }

    pub async fn get_liked_songs(&self, limit: u32) -> Result<Vec<SearchTrack>> {
        use futures::TryStreamExt;
        use futures::StreamExt;

        let tracks_stream = self.client.current_user_saved_tracks(None);
        let saved_tracks: Vec<_> = tracks_stream
            .take(limit as usize)
            .try_collect()
            .await?;

        let tracks: Vec<SearchTrack> = saved_tracks
            .into_iter()
            .map(|saved| {
                let track = saved.track;
                let track_id = track.id.as_ref().map(|id| id.id().to_string()).unwrap_or_default();
                let all_artists: Vec<String> = track.artists.iter().map(|a| a.name.clone()).collect();
                SearchTrack {
                    uri: format!("spotify:track:{}", track_id),
                    id: track_id,
                    name: track.name,
                    artist: track.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
                    artists: all_artists,
                    album: track.album.name,
                    duration_ms: track.duration.num_milliseconds() as u32,
                    liked: true, // These are liked songs by definition
                }
            })
            .collect();

        // Update the cache with all liked song IDs
        let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
        self.liked_songs_cache.update(track_ids).await;

        // Save cache to disk (async, don't block on errors)
        let cache = self.liked_songs_cache.clone();
        tokio::spawn(async move {
            let _ = cache.save_to_disk().await;
        });

        Ok(tracks)
    }

    pub async fn get_saved_albums(&self, limit: u32) -> Result<Vec<SearchAlbum>> {
        use futures::TryStreamExt;

        let albums_stream = self.client.current_user_saved_albums(None);
        let saved_albums: Vec<_> = albums_stream.try_collect().await?;

        let albums: Vec<SearchAlbum> = saved_albums
            .into_iter()
            .take(limit as usize)
            .map(|saved| {
                let album = saved.album;
                SearchAlbum {
                    id: album.id.id().to_string(),
                    name: album.name,
                    artist: album.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
                    year: album.release_date.chars().take(4).collect(),
                }
            })
            .collect();

        Ok(albums)
    }

    pub async fn get_followed_artists(&self, limit: u32) -> Result<Vec<SearchArtist>> {
        let result = self.client.current_user_followed_artists(None, Some(limit)).await?;

        let artists: Vec<SearchArtist> = result.items
            .into_iter()
            .map(|artist| SearchArtist {
                id: artist.id.id().to_string(),
                name: artist.name,
                genres: artist.genres,
            })
            .collect();

        Ok(artists)
    }

    pub async fn get_recently_played(&self, limit: u32) -> Result<Vec<SearchTrack>> {
        let history = self.client.current_user_recently_played(Some(limit), None).await?;

        let tracks: Vec<SearchTrack> = history.items
            .into_iter()
            .map(|item| {
                let track = item.track;
                let track_id = track.id.as_ref().map(|id| id.id().to_string()).unwrap_or_default();
                let all_artists: Vec<String> = track.artists.iter().map(|a| a.name.clone()).collect();
                SearchTrack {
                    uri: format!("spotify:track:{}", track_id),
                    id: track_id,
                    name: track.name,
                    artist: track.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
                    artists: all_artists,
                    album: track.album.name,
                    duration_ms: track.duration.num_milliseconds() as u32,
                    liked: false, // Set by mark_tracks_liked() in controller
                }
            })
            .collect();

        Ok(tracks)
    }
}
