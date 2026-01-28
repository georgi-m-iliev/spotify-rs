use std::sync::Arc;
use std::collections::HashSet;
use anyhow::Result;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tracing::{debug, info, trace};
use rspotify::{
    model::{CurrentPlaybackContext, PlayableItem, SearchType, Market, AlbumId, PlaylistId, ArtistId},
    prelude::*,
    AuthCodeSpotify,
};

const LIKED_SONGS_CACHE_FILE: &str = ".cache/liked_songs.json";

/// Cache for liked song IDs to enable fast lookup without API calls
#[derive(Clone)]
pub struct LikedSongsCache {
    /// Set of liked track IDs for O(1) lookup
    liked_ids: Arc<RwLock<HashSet<String>>>,
    /// Whether the cache has been loaded
    loaded: Arc<RwLock<bool>>,
}

impl LikedSongsCache {
    pub fn new() -> Self {
        Self {
            liked_ids: Arc::new(RwLock::new(HashSet::new())),
            loaded: Arc::new(RwLock::new(false)),
        }
    }

    /// Load the cache from disk
    pub async fn load_from_disk(&self) -> Result<()> {
        use std::fs;
        use std::path::Path;

        let path = Path::new(LIKED_SONGS_CACHE_FILE);
        if path.exists() {
            let content = fs::read_to_string(path)?;
            let ids: Vec<String> = serde_json::from_str(&content)?;
            let mut liked_ids = self.liked_ids.write().await;
            *liked_ids = ids.into_iter().collect();
            let mut loaded = self.loaded.write().await;
            *loaded = true;
        }
        Ok(())
    }

    /// Save the cache to disk
    pub async fn save_to_disk(&self) -> Result<()> {
        use std::fs;
        use std::path::Path;

        // Ensure .cache directory exists
        let cache_dir = Path::new(".cache");
        if !cache_dir.exists() {
            fs::create_dir_all(cache_dir)?;
        }

        let liked_ids = self.liked_ids.read().await;
        let ids: Vec<&String> = liked_ids.iter().collect();
        let content = serde_json::to_string(&ids)?;
        fs::write(LIKED_SONGS_CACHE_FILE, content)?;
        Ok(())
    }

    /// Update the cache with a new set of liked track IDs
    pub async fn update(&self, track_ids: Vec<String>) {
        let mut liked_ids = self.liked_ids.write().await;
        *liked_ids = track_ids.into_iter().collect();
        let mut loaded = self.loaded.write().await;
        *loaded = true;
    }

    /// Check if a track is liked
    pub async fn is_liked(&self, track_id: &str) -> bool {
        let liked_ids = self.liked_ids.read().await;
        liked_ids.contains(track_id)
    }

    /// Add a track to the liked cache
    pub async fn add(&self, track_id: String) {
        let mut liked_ids = self.liked_ids.write().await;
        liked_ids.insert(track_id);
    }

    /// Remove a track from the liked cache
    pub async fn remove(&self, track_id: &str) {
        let mut liked_ids = self.liked_ids.write().await;
        liked_ids.remove(track_id);
    }

    /// Check if the cache has been loaded
    pub async fn is_loaded(&self) -> bool {
        *self.loaded.read().await
    }

    /// Get the number of liked songs in the cache
    pub async fn len(&self) -> usize {
        self.liked_ids.read().await.len()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveSection {
    Search,
    Library,
    Playlists,
    MainContent,
}

impl ActiveSection {
    pub fn next(self) -> Self {
        match self {
            ActiveSection::Search => ActiveSection::Library,
            ActiveSection::Library => ActiveSection::Playlists,
            ActiveSection::Playlists => ActiveSection::MainContent,
            ActiveSection::MainContent => ActiveSection::Search,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            ActiveSection::Search => ActiveSection::MainContent,
            ActiveSection::Library => ActiveSection::Search,
            ActiveSection::Playlists => ActiveSection::Library,
            ActiveSection::MainContent => ActiveSection::Playlists,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LibraryItem {
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub is_active: bool,
    pub volume_percent: Option<u8>,
}

#[derive(Clone, Debug)]
pub struct PlaylistItem {
    pub id: String,
    pub uri: String,
    pub name: String,
}

#[derive(Clone)]
pub struct UiState {
    pub active_section: ActiveSection,
    pub search_query: String,
    pub library_items: Vec<LibraryItem>,
    pub library_selected: usize,
    pub playlists: Vec<PlaylistItem>,
    pub playlist_selected: usize,
    pub error_message: Option<String>,
    pub error_timestamp: Option<Instant>,
    // Device picker state
    pub show_device_picker: bool,
    pub available_devices: Vec<DeviceInfo>,
    pub device_selected: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepeatState {
    Off,
    All,
    One,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            active_section: ActiveSection::Search,
            search_query: String::new(),
            library_items: vec![
                LibraryItem { name: "Recently played".to_string() },
                LibraryItem { name: "Liked songs".to_string() },
                LibraryItem { name: "Albums".to_string() },
                LibraryItem { name: "Artists".to_string() },
            ],
            library_selected: 0,
            playlists: vec![], // Will be loaded from Spotify API
            playlist_selected: 0,
            error_message: None,
            error_timestamp: None,
            show_device_picker: false,
            available_devices: vec![],
            device_selected: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SearchTrack {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u32,
    pub uri: String,
    pub liked: bool,
}

/// An album from search results
#[derive(Clone, Debug)]
pub struct SearchAlbum {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub year: String,
    pub total_tracks: u32,
}

/// An artist from search results
#[derive(Clone, Debug)]
pub struct SearchArtist {
    pub id: String,
    pub name: String,
    pub genres: Vec<String>,
}

/// A playlist from search results
#[derive(Clone, Debug)]
pub struct SearchPlaylist {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub total_tracks: u32,
}

/// Combined search results
#[derive(Clone, Debug, Default)]
pub struct SearchResults {
    pub tracks: Vec<SearchTrack>,
    pub albums: Vec<SearchAlbum>,
    pub artists: Vec<SearchArtist>,
    pub playlists: Vec<SearchPlaylist>,
    pub best_match: SearchResultSection,
}

impl SearchResults {
    /// Determine the best matching category based on exact/close name matches with the query
    pub fn determine_best_match(&mut self, query: &str) {
        let query_lower = query.to_lowercase();

        // Score each category based on how well the top result matches the query
        // Higher score = better match
        // Artists have highest priority for exact matches (e.g., searching "Coldplay")
        let artist_score = self.artists.first().map(|a| {
            let name_lower = a.name.to_lowercase();
            if name_lower == query_lower { 100 }
            else if name_lower.starts_with(&query_lower) { 80 }
            else if name_lower.contains(&query_lower) { 60 }
            else { 0 }
        }).unwrap_or(0);

        // Tracks (songs) are second priority - most searches are for songs
        let track_score = self.tracks.first().map(|t| {
            let name_lower = t.name.to_lowercase();
            let artist_lower = t.artist.to_lowercase();
            if name_lower == query_lower || artist_lower == query_lower { 95 }
            else if name_lower.starts_with(&query_lower) || artist_lower.starts_with(&query_lower) { 75 }
            else if name_lower.contains(&query_lower) || artist_lower.contains(&query_lower) { 55 }
            else { 0 }
        }).unwrap_or(0);

        // Albums have lower priority than tracks
        let album_score = self.albums.first().map(|a| {
            let name_lower = a.name.to_lowercase();
            if name_lower == query_lower { 85 }
            else if name_lower.starts_with(&query_lower) { 65 }
            else if name_lower.contains(&query_lower) { 45 }
            else { 0 }
        }).unwrap_or(0);

        // Playlists have lowest priority
        let playlist_score = self.playlists.first().map(|p| {
            let name_lower = p.name.to_lowercase();
            if name_lower == query_lower { 80 }
            else if name_lower.starts_with(&query_lower) { 60 }
            else if name_lower.contains(&query_lower) { 40 }
            else { 0 }
        }).unwrap_or(0);

        // Find the best match, defaulting to tracks if all scores are 0
        let max_score = artist_score.max(album_score).max(playlist_score).max(track_score);

        self.best_match = if max_score == 0 {
            // No good match, default to tracks if available, otherwise first non-empty
            if !self.tracks.is_empty() { SearchResultSection::Tracks }
            else if !self.artists.is_empty() { SearchResultSection::Artists }
            else if !self.albums.is_empty() { SearchResultSection::Albums }
            else { SearchResultSection::Playlists }
        } else if artist_score == max_score {
            SearchResultSection::Artists
        } else if album_score == max_score {
            SearchResultSection::Albums
        } else if playlist_score == max_score {
            SearchResultSection::Playlists
        } else {
            SearchResultSection::Tracks
        };
    }
}

#[derive(Clone, Debug)]
pub struct AlbumDetail {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub year: String,
    pub tracks: Vec<SearchTrack>,
}

#[derive(Clone, Debug)]
pub struct PlaylistDetail {
    pub id: String,
    pub uri: String,
    pub name: String,
    pub owner: String,
    pub description: Option<String>,
    pub tracks: Vec<SearchTrack>,
    /// Total number of tracks in the playlist (may be more than loaded)
    pub total_tracks: u32,
    /// Whether there are more tracks to load
    pub has_more: bool,
    /// Whether more tracks are currently being loaded
    pub loading_more: bool,
}

#[derive(Clone, Debug)]
pub struct ArtistDetail {
    pub id: String,
    pub name: String,
    pub genres: Vec<String>,
    pub top_tracks: Vec<SearchTrack>,
    pub albums: Vec<SearchAlbum>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SearchResultSection {
    #[default]
    Tracks,
    Albums,
    Artists,
    Playlists,
}

impl SearchResultSection {
    pub fn next(self) -> Self {
        match self {
            Self::Tracks => Self::Albums,
            Self::Albums => Self::Artists,
            Self::Artists => Self::Playlists,
            Self::Playlists => Self::Tracks,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Tracks => Self::Playlists,
            Self::Albums => Self::Tracks,
            Self::Artists => Self::Albums,
            Self::Playlists => Self::Artists,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub enum ContentView {
    #[default]
    Empty,
    SearchResults {
        results: SearchResults,
        section: SearchResultSection,
        track_index: usize,
        album_index: usize,
        artist_index: usize,
        playlist_index: usize,
    },
    AlbumDetail {
        detail: AlbumDetail,
        selected_index: usize,
    },
    PlaylistDetail {
        detail: PlaylistDetail,
        selected_index: usize,
    },
    ArtistDetail {
        detail: ArtistDetail,
        section: ArtistDetailSection,
        track_index: usize,
        album_index: usize,
    },
    /// Liked songs view (library)
    LikedSongs {
        tracks: Vec<SearchTrack>,
        selected_index: usize,
    },
    /// Saved albums view (library)
    SavedAlbums {
        albums: Vec<SearchAlbum>,
        selected_index: usize,
    },
    /// Followed artists view (library)
    FollowedArtists {
        artists: Vec<SearchArtist>,
        selected_index: usize,
    },
    /// Recently played tracks view (library)
    RecentlyPlayed {
        tracks: Vec<SearchTrack>,
        selected_index: usize,
    },
}

/// Which section within artist detail is selected
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ArtistDetailSection {
    #[default]
    TopTracks,
    Albums,
}

impl ArtistDetailSection {
    pub fn next(self) -> Self {
        match self {
            Self::TopTracks => Self::Albums,
            Self::Albums => Self::TopTracks,
        }
    }
}

/// State for the main content area
#[derive(Clone, Debug, Default)]
pub struct ContentState {
    pub view: ContentView,
    pub navigation_stack: Vec<ContentView>,
    pub is_loading: bool,
}

#[derive(Clone)]
pub struct SpotifyClient {
    client: Arc<AuthCodeSpotify>,
    local_device_name: Option<String>,
    liked_songs_cache: LikedSongsCache,
}

impl SpotifyClient {
    pub fn new(client: AuthCodeSpotify, local_device_name: Option<String>) -> Self {
        Self {
            client: Arc::new(client),
            local_device_name,
            liked_songs_cache: LikedSongsCache::new(),
        }
    }

    /// Get a reference to the liked songs cache
    pub fn liked_cache(&self) -> &LikedSongsCache {
        &self.liked_songs_cache
    }

    /// Initialize the liked songs cache - load from disk and optionally refresh from API
    pub async fn init_liked_songs_cache(&self) -> Result<()> {
        // First try to load from disk
        if let Err(_e) = self.liked_songs_cache.load_from_disk().await {
            // Silently ignore - cache file may not exist yet
        }
        Ok(())
    }

    /// Refresh the liked songs cache from the API and save to disk
    pub async fn refresh_liked_songs_cache(&self) -> Result<()> {
        use futures::TryStreamExt;
        use futures::StreamExt;
        use rspotify::prelude::Id;

        debug!("Refreshing liked songs cache from API");

        // Fetch all liked songs from API
        let tracks_stream = self.client.current_user_saved_tracks(None);
        let saved_tracks: Vec<_> = tracks_stream
            .take(1000) // Reasonable limit
            .try_collect()
            .await?;

        let track_ids: Vec<String> = saved_tracks
            .into_iter()
            .filter_map(|saved| saved.track.id.map(|id| id.id().to_string()))
            .collect();

        info!(count = track_ids.len(), "Liked songs cache refreshed");

        // Update cache
        self.liked_songs_cache.update(track_ids).await;

        // Save to disk
        let _ = self.liked_songs_cache.save_to_disk().await;

        Ok(())
    }

    /// Check if a track is liked (uses cache for fast lookup)
    pub async fn is_track_liked(&self, track_id: &str) -> bool {
        self.liked_songs_cache.is_liked(track_id).await
    }

    /// Mark tracks with their liked status using the cache
    pub async fn mark_tracks_liked(&self, tracks: &mut [SearchTrack]) {
        for track in tracks.iter_mut() {
            track.liked = self.liked_songs_cache.is_liked(&track.id).await;
        }
    }

    pub async fn get_current_playback(&self) -> Result<Option<CurrentPlaybackContext>> {
        trace!("Fetching current playback state");
        let result = self.client.current_playback(None, None::<Vec<_>>).await?;
        if let Some(ref playback) = result {
            trace!(
                is_playing = playback.is_playing,
                device = ?playback.device.name,
                "Got playback state"
            );
        }
        Ok(result)
    }

    /// Get the device ID for the currently active device
    /// This respects device switching by using whatever device is currently active
    /// Falls back to the local device if no active device is found
    async fn get_device_id(&self) -> Option<String> {
        if let Ok(devices) = self.client.device().await {
            // First, try to find the active device
            let active_device = devices.iter().find(|d| d.is_active);
            if let Some(device) = active_device {
                debug!(device_name = %device.name, device_id = ?device.id, "Found active device");
                return device.id.clone();
            }

            // No active device - try to find our local device as fallback
            if let Some(local_name) = &self.local_device_name {
                let local_device = devices.iter().find(|d| &d.name == local_name);
                if let Some(device) = local_device {
                    debug!(device_name = %device.name, device_id = ?device.id, "No active device, using local device as fallback");
                    return device.id.clone();
                }
            }

            debug!(available_devices = devices.len(), "No active device found and local device not in list");
            None
        } else {
            debug!("Failed to get devices list");
            None
        }
    }

    /// Get the local librespot device name
    pub fn get_local_device_name(&self) -> Option<&str> {
        self.local_device_name.as_deref()
    }

    pub async fn play(&self) -> Result<()> {
        let device_id = self.get_device_id().await;
        debug!(device_id = ?device_id, "API: resume_playback");
        self.client
            .resume_playback(device_id.as_deref(), None)
            .await?;
        Ok(())
    }

    pub async fn pause(&self) -> Result<()> {
        let device_id = self.get_device_id().await;
        debug!(device_id = ?device_id, "API: pause_playback");
        self.client.pause_playback(device_id.as_deref()).await?;
        Ok(())
    }

    pub async fn next_track(&self) -> Result<()> {
        let device_id = self.get_device_id().await;
        debug!(device_id = ?device_id, "API: next_track");
        self.client.next_track(device_id.as_deref()).await?;
        Ok(())
    }

    pub async fn previous_track(&self) -> Result<()> {
        let device_id = self.get_device_id().await;
        debug!(device_id = ?device_id, "API: previous_track");
        self.client.previous_track(device_id.as_deref()).await?;
        Ok(())
    }

    pub async fn set_shuffle(&self, state: bool) -> Result<()> {
        let device_id = self.get_device_id().await;
        debug!(state, device_id = ?device_id, "API: set_shuffle");
        self.client.shuffle(state, device_id.as_deref()).await?;
        Ok(())
    }

    pub async fn set_repeat(&self, state: RepeatState) -> Result<()> {
        let device_id = self.get_device_id().await;
        debug!(state = ?state, device_id = ?device_id, "API: set_repeat");
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
        debug!(volume, device_id = ?device_id, "API: set_volume");
        self.client.volume(volume, device_id.as_deref()).await?;
        Ok(())
    }

    /// Get list of available playback devices
    pub async fn get_available_devices(&self) -> Result<Vec<DeviceInfo>> {
        debug!("API: get_available_devices");
        let devices = self.client.device().await?;
        let device_infos: Vec<DeviceInfo> = devices
            .into_iter()
            .map(|d| DeviceInfo {
                id: d.id.unwrap_or_default(),
                name: d.name,
                is_active: d.is_active,
                volume_percent: d.volume_percent.map(|v| v as u8),
            })
            .collect();
        debug!(count = device_infos.len(), "Found devices");
        Ok(device_infos)
    }

    /// Check if there's any active device available for playback
    pub async fn has_active_device(&self) -> bool {
        if let Ok(devices) = self.client.device().await {
            devices.iter().any(|d| d.is_active)
        } else {
            false
        }
    }

    /// Transfer playback to a specific device
    pub async fn transfer_playback_to_device(&self, device_id: &str, start_playing: bool) -> Result<()> {
        debug!(device_id, start_playing, "API: transfer_playback");
        self.client
            .transfer_playback(device_id, Some(start_playing))
            .await?;
        Ok(())
    }

    pub async fn transfer_playback(&self) -> Result<()> {
        if let Some(device_id) = self.get_device_id().await {
            self.client
                .transfer_playback(&device_id, Some(true))
                .await?;
        }
        Ok(())
    }

    /// Search for tracks, albums, artists, and playlists
    pub async fn search(&self, query: &str, limit: u32) -> Result<SearchResults> {
        use rspotify::prelude::Id;

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
                results.tracks.push(SearchTrack {
                    uri: format!("spotify:track:{}", track_id),
                    id: track_id,
                    name: track.name,
                    artist: track.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
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
                    total_tracks: 0,
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

    /// Get album details with tracks
    pub async fn get_album(&self, album_id: &str) -> Result<AlbumDetail> {
        use rspotify::prelude::Id;

        let id = AlbumId::from_id(album_id)?;
        let album = self.client.album(id.clone(), None).await?;

        let mut tracks = Vec::new();

        // Album tracks are included in the full album response
        for track in album.tracks.items.iter() {
            let track_id = track.id.as_ref().map(|id| id.id().to_string()).unwrap_or_default();
            tracks.push(SearchTrack {
                uri: format!("spotify:track:{}", track_id),
                id: track_id,
                name: track.name.clone(),
                artist: track.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
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

    /// Number of tracks to load per page
    pub const PLAYLIST_PAGE_SIZE: usize = 100;

    /// Get playlist details with first batch of tracks (paginated)
    pub async fn get_playlist(&self, playlist_id: &str) -> Result<PlaylistDetail> {
        self.get_playlist_with_offset(playlist_id, 0).await
    }

    /// Get playlist details with tracks starting from a specific offset
    pub async fn get_playlist_with_offset(&self, playlist_id: &str, offset: usize) -> Result<PlaylistDetail> {
        use futures::TryStreamExt;
        use futures::StreamExt;
        use rspotify::prelude::Id;

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
                tracks.push(SearchTrack {
                    uri: format!("spotify:track:{}", track_id),
                    id: track_id,
                    name: track.name.clone(),
                    artist: track.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
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
            description: playlist.description.clone(),
            tracks,
            total_tracks,
            has_more,
            loading_more: false,
        })
    }

    /// Load more tracks for a playlist (for pagination)
    pub async fn get_more_playlist_tracks(&self, playlist_id: &str, offset: usize) -> Result<(Vec<SearchTrack>, u32, bool)> {
        use futures::TryStreamExt;
        use futures::StreamExt;
        use rspotify::prelude::Id;

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
                tracks.push(SearchTrack {
                    uri: format!("spotify:track:{}", track_id),
                    id: track_id,
                    name: track.name.clone(),
                    artist: track.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
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

    /// Get artist details with top tracks and albums
    pub async fn get_artist(&self, artist_id: &str) -> Result<ArtistDetail> {
        use futures::TryStreamExt;
        use rspotify::prelude::Id;

        let id = ArtistId::from_id(artist_id)?;
        let artist = self.client.artist(id.clone()).await?;

        // Get top tracks - use FromToken to use user's account country
        let market = Market::FromToken;
        let top_tracks_result = self.client.artist_top_tracks(id.clone(), Some(market)).await?;

        let top_tracks: Vec<SearchTrack> = top_tracks_result
            .into_iter()
            .map(|track| {
                let track_id = track.id.as_ref().map(|id| id.id().to_string()).unwrap_or_default();
                SearchTrack {
                    uri: format!("spotify:track:{}", track_id),
                    id: track_id,
                    name: track.name,
                    artist: track.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
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
                total_tracks: 0,
            })
            .collect();

        Ok(ArtistDetail {
            id: artist_id.to_string(),
            name: artist.name,
            genres: artist.genres,
            top_tracks,
            albums,
        })
    }

    /// Play a specific track URI
    pub async fn play_track(&self, uri: &str) -> Result<()> {
        let device_id = self.get_device_id().await;
        debug!(uri, device_id = ?device_id, "API: play_track");

        // Extract track ID from URI (format: spotify:track:ID)
        let track_id = uri.split(':').last().unwrap_or(uri);

        self.client
            .start_uris_playback(
                [PlayableId::Track(rspotify::model::TrackId::from_id(track_id)?)],
                device_id.as_deref(),
                None,
                None,
            )
            .await?;
        Ok(())
    }

    /// Play a context (album, playlist, artist)
    pub async fn play_context(&self, uri: &str) -> Result<()> {
        let device_id = self.get_device_id().await;

        // Parse the URI to determine type
        let play_context = if uri.contains(":album:") {
            let id = uri.split(':').last().unwrap_or("");
            PlayContextId::Album(AlbumId::from_id(id)?)
        } else if uri.contains(":playlist:") {
            let id = uri.split(':').last().unwrap_or("");
            PlayContextId::Playlist(PlaylistId::from_id(id)?)
        } else if uri.contains(":artist:") {
            let id = uri.split(':').last().unwrap_or("");
            PlayContextId::Artist(ArtistId::from_id(id)?)
        } else {
            return Err(anyhow::anyhow!("Unknown context type: {}", uri));
        };

        self.client
            .start_context_playback(play_context, device_id.as_deref(), None, None)
            .await?;
        Ok(())
    }

    /// Play a context starting from a specific track (by URI)
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

    /// Get current user's playlists
    pub async fn get_user_playlists(&self, limit: u32) -> Result<Vec<PlaylistItem>> {
        use futures::TryStreamExt;
        use rspotify::prelude::Id;

        let playlist_stream = self.client.current_user_playlists();
        let all_playlists: Vec<_> = playlist_stream.try_collect().await?;

        let playlists: Vec<PlaylistItem> = all_playlists
            .into_iter()
            .take(limit as usize)
            .map(|playlist| {
                let id = playlist.id.id().to_string();
                PlaylistItem {
                    id: id.clone(),
                    uri: format!("spotify:playlist:{}", id),
                    name: playlist.name,
                }
            })
            .collect();

        Ok(playlists)
    }

    /// Get user's liked songs (saved tracks) and refresh the cache
    pub async fn get_liked_songs(&self, limit: u32) -> Result<Vec<SearchTrack>> {
        use futures::TryStreamExt;
        use futures::StreamExt;
        use rspotify::prelude::Id;

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
                SearchTrack {
                    uri: format!("spotify:track:{}", track_id),
                    id: track_id,
                    name: track.name,
                    artist: track.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
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

    /// Get user's saved albums
    pub async fn get_saved_albums(&self, limit: u32) -> Result<Vec<SearchAlbum>> {
        use futures::TryStreamExt;
        use rspotify::prelude::Id;

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
                    total_tracks: album.tracks.total,
                }
            })
            .collect();

        Ok(albums)
    }

    /// Get user's followed artists
    pub async fn get_followed_artists(&self, limit: u32) -> Result<Vec<SearchArtist>> {
        use rspotify::prelude::Id;

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

    /// Get recently played tracks
    pub async fn get_recently_played(&self, limit: u32) -> Result<Vec<SearchTrack>> {
        use rspotify::prelude::Id;

        let history = self.client.current_user_recently_played(Some(limit), None).await?;

        let tracks: Vec<SearchTrack> = history.items
            .into_iter()
            .map(|item| {
                let track = item.track;
                let track_id = track.id.as_ref().map(|id| id.id().to_string()).unwrap_or_default();
                SearchTrack {
                    uri: format!("spotify:track:{}", track_id),
                    id: track_id,
                    name: track.name,
                    artist: track.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
                    album: track.album.name,
                    duration_ms: track.duration.num_milliseconds() as u32,
                    liked: false, // Set by mark_tracks_liked() in controller
                }
            })
            .collect();

        Ok(tracks)
    }
}

#[derive(Clone, Debug)]
pub struct TrackMetadata {
    pub name: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u32,
    pub uri: String,
}

impl Default for TrackMetadata {
    fn default() -> Self {
        Self {
            name: "No track playing".to_string(),
            artist: String::new(),
            album: String::new(),
            duration_ms: 0,
            uri: String::new(),
        }
    }
}

impl TrackMetadata {
    pub fn from_playback(playback: &CurrentPlaybackContext) -> Self {
        if let Some(item) = &playback.item {
            match item {
                PlayableItem::Track(track) => {
                    let artist = track
                        .artists
                        .first()
                        .map(|a| a.name.clone())
                        .unwrap_or_default();

                    let uri = track.id.as_ref()
                        .map(|id| format!("spotify:track:{}", id.id()))
                        .unwrap_or_default();

                    Self {
                        name: track.name.clone(),
                        artist,
                        album: track.album.name.clone(),
                        duration_ms: track.duration.num_milliseconds() as u32,
                        uri,
                    }
                }
                PlayableItem::Episode(episode) => {
                    let uri = format!("spotify:episode:{}", episode.id.id());
                    Self {
                        name: episode.name.clone(),
                        artist: episode.show.name.clone(),
                        album: "Podcast".to_string(),
                        duration_ms: episode.duration.num_milliseconds() as u32,
                        uri,
                    }
                }
                PlayableItem::Unknown(_) => Self::default(),
            }
        } else {
            Self::default()
        }
    }
}

#[derive(Clone)]
struct PlaybackTiming {
    position_ms: u32,
    last_update: Instant,
    is_playing: bool,
    duration_ms: u32,
}

impl Default for PlaybackTiming {
    fn default() -> Self {
        Self {
            position_ms: 0,
            last_update: Instant::now(),
            is_playing: false,
            duration_ms: 0,
        }
    }
}

impl PlaybackTiming {
    fn current_position_ms(&self) -> u32 {
        if self.is_playing && self.duration_ms > 0 {
            let elapsed = self.last_update.elapsed().as_millis() as u32;
            self.position_ms.saturating_add(elapsed).min(self.duration_ms)
        } else {
            self.position_ms.min(self.duration_ms.max(1) - 1)
        }
    }

    fn update_position(&mut self, new_position_ms: u32, is_playing: bool) {
        let current_calculated = self.current_position_ms();
        let diff = new_position_ms as i64 - current_calculated as i64;

        let state_changed = self.is_playing != is_playing;
        let significant_backward_jump = diff < -2000;
        let significant_forward_jump = diff > 2000;
        let was_paused = !self.is_playing;
        let acceptable_sync = diff >= -100;

        if state_changed || significant_backward_jump || significant_forward_jump || was_paused || acceptable_sync {
            self.position_ms = new_position_ms;
            self.last_update = Instant::now();
        }
        self.is_playing = is_playing;
    }
}

#[derive(Clone, Debug)]
pub struct PlaybackSettings {
    pub device_name: String,
    pub shuffle: bool,
    pub repeat: RepeatState,
    pub volume: u8,
}

impl Default for PlaybackSettings {
    fn default() -> Self {
        Self {
            device_name: "spotify-rs".to_string(),
            shuffle: false,
            repeat: RepeatState::Off,
            volume: 70,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PlaybackInfo {
    pub track: TrackMetadata,
    pub progress_ms: u32,
    pub duration_ms: u32,
    pub is_playing: bool,
    pub settings: PlaybackSettings,
}

impl Default for PlaybackInfo {
    fn default() -> Self {
        Self {
            track: TrackMetadata::default(),
            progress_ms: 0,
            duration_ms: 0,
            is_playing: false,
            settings: PlaybackSettings::default(),
        }
    }
}

pub struct AppModel {
    pub spotify: Option<SpotifyClient>,
    track_metadata: Arc<Mutex<TrackMetadata>>,
    playback_timing: Arc<Mutex<PlaybackTiming>>,
    playback_settings: Arc<Mutex<PlaybackSettings>>,
    pub ui_state: Arc<Mutex<UiState>>,
    pub content_state: Arc<Mutex<ContentState>>,
    pub should_quit: Arc<Mutex<bool>>,
}

impl AppModel {
    pub fn new() -> Self {
        Self {
            spotify: None,
            track_metadata: Arc::new(Mutex::new(TrackMetadata::default())),
            playback_timing: Arc::new(Mutex::new(PlaybackTiming::default())),
            playback_settings: Arc::new(Mutex::new(PlaybackSettings::default())),
            ui_state: Arc::new(Mutex::new(UiState::default())),
            content_state: Arc::new(Mutex::new(ContentState::default())),
            should_quit: Arc::new(Mutex::new(false)),
        }
    }

    pub fn set_spotify_client(&mut self, client: SpotifyClient) {
        self.spotify = Some(client);
    }

    pub async fn update_device_name(&self, name: String) {
        let mut settings = self.playback_settings.lock().await;
        settings.device_name = name;
    }

    pub async fn update_track_info(&self, track: TrackMetadata) {
        let duration_ms = track.duration_ms;
        *self.track_metadata.lock().await = track;

        let mut timing = self.playback_timing.lock().await;
        timing.duration_ms = duration_ms;
    }

    pub async fn update_playback_position(&self, position_ms: u32, is_playing: bool) {
        let mut timing = self.playback_timing.lock().await;
        timing.update_position(position_ms, is_playing);
    }

    pub async fn set_playing(&self, is_playing: bool) {
        let mut timing = self.playback_timing.lock().await;
        timing.position_ms = timing.current_position_ms();
        timing.is_playing = is_playing;
        timing.last_update = Instant::now();
    }

    pub async fn update_from_playback_context(&self, playback: &CurrentPlaybackContext) {
        let track = TrackMetadata::from_playback(playback);
        let progress_ms = playback
            .progress
            .map(|d| d.num_milliseconds() as u32)
            .unwrap_or(0);
        let is_playing = playback.is_playing;

        *self.track_metadata.lock().await = track.clone();

        let mut timing = self.playback_timing.lock().await;
        timing.position_ms = progress_ms;
        timing.duration_ms = track.duration_ms;
        timing.is_playing = is_playing;
        timing.last_update = Instant::now();
        drop(timing);

        // Update shuffle and repeat states from playback context
        let mut settings = self.playback_settings.lock().await;
        settings.shuffle = playback.shuffle_state;
        settings.repeat = match playback.repeat_state {
            rspotify::model::RepeatState::Off => RepeatState::Off,
            rspotify::model::RepeatState::Track => RepeatState::One,
            rspotify::model::RepeatState::Context => RepeatState::All,
        };
        // Update volume if available from device info
        if let Some(volume) = playback.device.volume_percent {
            settings.volume = volume as u8;
        }
    }

    pub async fn get_playback_info(&self) -> PlaybackInfo {
        let track = self.track_metadata.lock().await.clone();
        let timing = self.playback_timing.lock().await;
        let settings = self.playback_settings.lock().await.clone();

        PlaybackInfo {
            track,
            progress_ms: timing.current_position_ms(),
            duration_ms: timing.duration_ms,
            is_playing: timing.is_playing,
            settings,
        }
    }

    pub async fn is_playing(&self) -> bool {
        self.playback_timing.lock().await.is_playing
    }

    pub async fn get_shuffle_state(&self) -> bool {
        self.playback_settings.lock().await.shuffle
    }

    pub async fn set_shuffle(&self, shuffle: bool) {
        let mut settings = self.playback_settings.lock().await;
        settings.shuffle = shuffle;
    }

    pub async fn get_repeat_state(&self) -> RepeatState {
        self.playback_settings.lock().await.repeat
    }

    pub async fn set_repeat(&self, repeat: RepeatState) {
        let mut settings = self.playback_settings.lock().await;
        settings.repeat = repeat;
    }

    pub async fn get_volume(&self) -> u8 {
        self.playback_settings.lock().await.volume
    }

    pub async fn set_volume(&self, volume: u8) {
        let mut settings = self.playback_settings.lock().await;
        settings.volume = volume;
    }

    pub async fn should_quit(&self) -> bool {
        *self.should_quit.lock().await
    }

    pub async fn set_should_quit(&self, quit: bool) {
        *self.should_quit.lock().await = quit;
    }

    pub async fn get_ui_state(&self) -> UiState {
        self.ui_state.lock().await.clone()
    }

    pub async fn cycle_section_forward(&self) {
        let mut state = self.ui_state.lock().await;
        state.active_section = state.active_section.next();
    }

    pub async fn cycle_section_backward(&self) {
        let mut state = self.ui_state.lock().await;
        state.active_section = state.active_section.prev();
    }

    pub async fn move_selection_up(&self) {
        let mut state = self.ui_state.lock().await;
        match state.active_section {
            ActiveSection::Library => {
                if state.library_selected > 0 {
                    state.library_selected -= 1;
                }
            }
            ActiveSection::Playlists => {
                if state.playlist_selected > 0 {
                    state.playlist_selected -= 1;
                }
            }
            _ => {}
        }
    }

    pub async fn move_selection_down(&self) {
        let mut state = self.ui_state.lock().await;
        match state.active_section {
            ActiveSection::Library => {
                if state.library_selected < state.library_items.len().saturating_sub(1) {
                    state.library_selected += 1;
                }
            }
            ActiveSection::Playlists => {
                if state.playlist_selected < state.playlists.len().saturating_sub(1) {
                    state.playlist_selected += 1;
                }
            }
            _ => {}
        }
    }

    pub async fn update_search_query(&self, query: String) {
        let mut state = self.ui_state.lock().await;
        state.search_query = query;
    }

    pub async fn append_to_search(&self, c: char) {
        let mut state = self.ui_state.lock().await;
        state.search_query.push(c);
    }

    pub async fn backspace_search(&self) {
        let mut state = self.ui_state.lock().await;
        state.search_query.pop();
    }

    pub async fn set_playlists(&self, playlists: Vec<PlaylistItem>) {
        let mut state = self.ui_state.lock().await;
        state.playlists = playlists;
        state.playlist_selected = 0;
    }

    pub async fn get_selected_playlist(&self) -> Option<PlaylistItem> {
        let state = self.ui_state.lock().await;
        state.playlists.get(state.playlist_selected).cloned()
    }

    pub async fn set_error(&self, message: String) {
        let mut state = self.ui_state.lock().await;
        state.error_message = Some(message);
        state.error_timestamp = Some(Instant::now());
    }

    pub async fn clear_error(&self) {
        let mut state = self.ui_state.lock().await;
        state.error_message = None;
        state.error_timestamp = None;
    }

    pub async fn auto_clear_old_errors(&self) {
        let mut state = self.ui_state.lock().await;
        if let Some(timestamp) = state.error_timestamp {
            if timestamp.elapsed().as_secs() > 5 {
                state.error_message = None;
                state.error_timestamp = None;
            }
        }
    }

    // ========================================================================
    // Device Picker Management
    // ========================================================================

    pub async fn show_device_picker(&self, devices: Vec<DeviceInfo>) {
        let mut state = self.ui_state.lock().await;
        // Find currently active device and select it
        let active_index = devices.iter().position(|d| d.is_active).unwrap_or(0);
        state.available_devices = devices;
        state.device_selected = active_index;
        state.show_device_picker = true;
    }

    pub async fn hide_device_picker(&self) {
        let mut state = self.ui_state.lock().await;
        state.show_device_picker = false;
    }

    pub async fn is_device_picker_open(&self) -> bool {
        self.ui_state.lock().await.show_device_picker
    }

    pub async fn device_picker_move_up(&self) {
        let mut state = self.ui_state.lock().await;
        if state.device_selected > 0 {
            state.device_selected -= 1;
        }
    }

    pub async fn device_picker_move_down(&self) {
        let mut state = self.ui_state.lock().await;
        if state.device_selected < state.available_devices.len().saturating_sub(1) {
            state.device_selected += 1;
        }
    }

    pub async fn get_selected_device(&self) -> Option<DeviceInfo> {
        let state = self.ui_state.lock().await;
        state.available_devices.get(state.device_selected).cloned()
    }

    pub async fn get_local_device_name(&self) -> String {
        self.playback_settings.lock().await.device_name.clone()
    }

    // ========================================================================
    // Content State Management
    // ========================================================================

    pub async fn get_content_state(&self) -> ContentState {
        self.content_state.lock().await.clone()
    }

    pub async fn set_search_results(&self, results: SearchResults) {
        let mut state = self.content_state.lock().await;
        // Push current view to navigation stack if it's not empty
        if !matches!(state.view, ContentView::Empty) {
            state.navigation_stack.clear(); // Clear stack on new search
        }
        // Use the best matching category as the initial section
        let initial_section = results.best_match;
        state.view = ContentView::SearchResults {
            results,
            section: initial_section,
            track_index: 0,
            album_index: 0,
            artist_index: 0,
            playlist_index: 0,
        };
        state.is_loading = false;
    }

    pub async fn set_album_detail(&self, detail: AlbumDetail) {
        let mut state = self.content_state.lock().await;
        // Save current view to navigation stack before navigating away
        if !matches!(state.view, ContentView::Empty) {
            let previous_view = state.view.clone();
            state.navigation_stack.push(previous_view);
        }
        state.view = ContentView::AlbumDetail {
            detail,
            selected_index: 0,
        };
        state.is_loading = false;
    }

    pub async fn set_playlist_detail(&self, detail: PlaylistDetail) {
        let mut state = self.content_state.lock().await;
        // Save current view to navigation stack before navigating away
        if !matches!(state.view, ContentView::Empty) {
            let previous_view = state.view.clone();
            state.navigation_stack.push(previous_view);
        }
        state.view = ContentView::PlaylistDetail {
            detail,
            selected_index: 0,
        };
        state.is_loading = false;
    }

    pub async fn set_artist_detail(&self, detail: ArtistDetail) {
        let mut state = self.content_state.lock().await;
        // Save current view to navigation stack before navigating away
        if !matches!(state.view, ContentView::Empty) {
            let previous_view = state.view.clone();
            state.navigation_stack.push(previous_view);
        }
        state.view = ContentView::ArtistDetail {
            detail,
            section: ArtistDetailSection::TopTracks,
            track_index: 0,
            album_index: 0,
        };
        state.is_loading = false;
    }

    pub async fn set_liked_songs(&self, tracks: Vec<SearchTrack>) {
        let mut state = self.content_state.lock().await;
        state.navigation_stack.clear();
        state.view = ContentView::LikedSongs {
            tracks,
            selected_index: 0,
        };
        state.is_loading = false;
    }

    pub async fn set_saved_albums(&self, albums: Vec<SearchAlbum>) {
        let mut state = self.content_state.lock().await;
        state.navigation_stack.clear();
        state.view = ContentView::SavedAlbums {
            albums,
            selected_index: 0,
        };
        state.is_loading = false;
    }

    pub async fn set_followed_artists(&self, artists: Vec<SearchArtist>) {
        let mut state = self.content_state.lock().await;
        state.navigation_stack.clear();
        state.view = ContentView::FollowedArtists {
            artists,
            selected_index: 0,
        };
        state.is_loading = false;
    }

    pub async fn set_recently_played(&self, tracks: Vec<SearchTrack>) {
        let mut state = self.content_state.lock().await;
        state.navigation_stack.clear();
        state.view = ContentView::RecentlyPlayed {
            tracks,
            selected_index: 0,
        };
        state.is_loading = false;
    }

    pub async fn set_content_loading(&self, loading: bool) {
        let mut state = self.content_state.lock().await;
        state.is_loading = loading;
    }

    pub async fn navigate_back(&self) -> bool {
        let mut state = self.content_state.lock().await;
        if let Some(previous_view) = state.navigation_stack.pop() {
            // Restore the previous view
            state.view = previous_view;
            true
        } else {
            // No navigation history - go back to empty view
            state.view = ContentView::Empty;
            false
        }
    }

    /// Navigate within search results sections (left/right)
    pub async fn navigate_search_section(&self, forward: bool) {
        let mut state = self.content_state.lock().await;
        match &mut state.view {
            ContentView::SearchResults { section, .. } => {
                *section = if forward { section.next() } else { section.prev() };
            }
            ContentView::ArtistDetail { section, .. } => {
                // Artist detail only has 2 sections, so forward/backward both toggle
                *section = section.next();
            }
            _ => {}
        }
    }

    /// Navigate within artist detail sections
    pub async fn navigate_artist_section(&self) {
        let mut state = self.content_state.lock().await;
        if let ContentView::ArtistDetail { ref mut section, .. } = state.view {
            *section = section.next();
        }
    }

    /// Move selection up in current content view
    pub async fn content_move_up(&self) {
        let mut state = self.content_state.lock().await;
        match &mut state.view {
            ContentView::SearchResults {
                section,
                track_index,
                album_index,
                artist_index,
                playlist_index,
                ..
            } => {
                let idx = match section {
                    SearchResultSection::Tracks => track_index,
                    SearchResultSection::Albums => album_index,
                    SearchResultSection::Artists => artist_index,
                    SearchResultSection::Playlists => playlist_index,
                };
                if *idx > 0 {
                    *idx -= 1;
                }
            }
            ContentView::AlbumDetail { selected_index, .. } => {
                if *selected_index > 0 {
                    *selected_index -= 1;
                }
            }
            ContentView::PlaylistDetail { selected_index, .. } => {
                if *selected_index > 0 {
                    *selected_index -= 1;
                }
            }
            ContentView::ArtistDetail {
                section,
                track_index,
                album_index,
                ..
            } => {
                let idx = match section {
                    ArtistDetailSection::TopTracks => track_index,
                    ArtistDetailSection::Albums => album_index,
                };
                if *idx > 0 {
                    *idx -= 1;
                }
            }
            ContentView::LikedSongs { selected_index, .. }
            | ContentView::RecentlyPlayed { selected_index, .. } => {
                if *selected_index > 0 {
                    *selected_index -= 1;
                }
            }
            ContentView::SavedAlbums { selected_index, .. } => {
                if *selected_index > 0 {
                    *selected_index -= 1;
                }
            }
            ContentView::FollowedArtists { selected_index, .. } => {
                if *selected_index > 0 {
                    *selected_index -= 1;
                }
            }
            ContentView::Empty => {}
        }
    }

    /// Move selection down in current content view
    pub async fn content_move_down(&self) {
        let mut state = self.content_state.lock().await;
        match &mut state.view {
            ContentView::SearchResults {
                results,
                section,
                track_index,
                album_index,
                artist_index,
                playlist_index,
            } => {
                let (idx, max) = match section {
                    SearchResultSection::Tracks => (track_index, results.tracks.len()),
                    SearchResultSection::Albums => (album_index, results.albums.len()),
                    SearchResultSection::Artists => (artist_index, results.artists.len()),
                    SearchResultSection::Playlists => (playlist_index, results.playlists.len()),
                };
                if *idx < max.saturating_sub(1) {
                    *idx += 1;
                }
            }
            ContentView::AlbumDetail { detail, selected_index } => {
                if *selected_index < detail.tracks.len().saturating_sub(1) {
                    *selected_index += 1;
                }
            }
            ContentView::PlaylistDetail { detail, selected_index } => {
                if *selected_index < detail.tracks.len().saturating_sub(1) {
                    *selected_index += 1;
                }
            }
            ContentView::ArtistDetail {
                detail,
                section,
                track_index,
                album_index,
            } => {
                let (idx, max) = match section {
                    ArtistDetailSection::TopTracks => (track_index, detail.top_tracks.len()),
                    ArtistDetailSection::Albums => (album_index, detail.albums.len()),
                };
                if *idx < max.saturating_sub(1) {
                    *idx += 1;
                }
            }
            ContentView::LikedSongs { tracks, selected_index } => {
                if *selected_index < tracks.len().saturating_sub(1) {
                    *selected_index += 1;
                }
            }
            ContentView::RecentlyPlayed { tracks, selected_index } => {
                if *selected_index < tracks.len().saturating_sub(1) {
                    *selected_index += 1;
                }
            }
            ContentView::SavedAlbums { albums, selected_index } => {
                if *selected_index < albums.len().saturating_sub(1) {
                    *selected_index += 1;
                }
            }
            ContentView::FollowedArtists { artists, selected_index } => {
                if *selected_index < artists.len().saturating_sub(1) {
                    *selected_index += 1;
                }
            }
            ContentView::Empty => {}
        }
    }

    /// Get the currently selected item info for actions (play, open)
    pub async fn get_selected_content_item(&self) -> Option<SelectedItem> {
        let state = self.content_state.lock().await;
        match &state.view {
            ContentView::SearchResults {
                results,
                section,
                track_index,
                album_index,
                artist_index,
                playlist_index,
            } => match section {
                SearchResultSection::Tracks => results.tracks.get(*track_index).map(|t| {
                    SelectedItem::Track {
                        uri: t.uri.clone(),
                        name: t.name.clone(),
                    }
                }),
                SearchResultSection::Albums => results.albums.get(*album_index).map(|a| {
                    SelectedItem::Album {
                        id: a.id.clone(),
                        name: a.name.clone(),
                    }
                }),
                SearchResultSection::Artists => results.artists.get(*artist_index).map(|a| {
                    SelectedItem::Artist {
                        id: a.id.clone(),
                        name: a.name.clone(),
                    }
                }),
                SearchResultSection::Playlists => results.playlists.get(*playlist_index).map(|p| {
                    SelectedItem::Playlist {
                        id: p.id.clone(),
                        name: p.name.clone(),
                    }
                }),
            },
            ContentView::AlbumDetail { detail, selected_index } => {
                detail.tracks.get(*selected_index).map(|t| SelectedItem::AlbumTrack {
                    album_uri: format!("spotify:album:{}", detail.id),
                    track_uri: t.uri.clone(),
                    name: t.name.clone(),
                })
            }
            ContentView::PlaylistDetail { detail, selected_index } => {
                detail.tracks.get(*selected_index).map(|t| SelectedItem::PlaylistTrack {
                    playlist_uri: detail.uri.clone(),
                    track_uri: t.uri.clone(),
                    name: t.name.clone(),
                })
            }
            ContentView::ArtistDetail {
                detail,
                section,
                track_index,
                album_index,
            } => match section {
                ArtistDetailSection::TopTracks => detail.top_tracks.get(*track_index).map(|t| {
                    SelectedItem::Track {
                        uri: t.uri.clone(),
                        name: t.name.clone(),
                    }
                }),
                ArtistDetailSection::Albums => detail.albums.get(*album_index).map(|a| {
                    SelectedItem::Album {
                        id: a.id.clone(),
                        name: a.name.clone(),
                    }
                }),
            },
            ContentView::LikedSongs { tracks, selected_index } => {
                tracks.get(*selected_index).map(|t| SelectedItem::Track {
                    uri: t.uri.clone(),
                    name: t.name.clone(),
                })
            }
            ContentView::RecentlyPlayed { tracks, selected_index } => {
                tracks.get(*selected_index).map(|t| SelectedItem::Track {
                    uri: t.uri.clone(),
                    name: t.name.clone(),
                })
            }
            ContentView::SavedAlbums { albums, selected_index } => {
                albums.get(*selected_index).map(|a| SelectedItem::Album {
                    id: a.id.clone(),
                    name: a.name.clone(),
                })
            }
            ContentView::FollowedArtists { artists, selected_index } => {
                artists.get(*selected_index).map(|a| SelectedItem::Artist {
                    id: a.id.clone(),
                    name: a.name.clone(),
                })
            }
            ContentView::Empty => None,
        }
    }

    /// Threshold for triggering loading more tracks (load when within this many tracks of the end)
    const PAGINATION_THRESHOLD: usize = 10;

    /// Check if we should load more playlist tracks (returns playlist_id and current offset if needed)
    pub async fn should_load_more_playlist_tracks(&self) -> Option<(String, usize)> {
        let state = self.content_state.lock().await;
        if let ContentView::PlaylistDetail { detail, selected_index } = &state.view {
            // Don't load if already loading or no more tracks
            if detail.loading_more || !detail.has_more {
                return None;
            }

            // Check if we're within threshold of the end
            let loaded_count = detail.tracks.len();
            if *selected_index + Self::PAGINATION_THRESHOLD >= loaded_count {
                return Some((detail.id.clone(), loaded_count));
            }
        }
        None
    }

    /// Set the loading_more flag for the current playlist
    pub async fn set_playlist_loading_more(&self, loading: bool) {
        let mut state = self.content_state.lock().await;
        if let ContentView::PlaylistDetail { detail, .. } = &mut state.view {
            detail.loading_more = loading;
        }
    }

    /// Append more tracks to the current playlist view
    pub async fn append_playlist_tracks(&self, mut new_tracks: Vec<SearchTrack>, has_more: bool) {
        let mut state = self.content_state.lock().await;
        if let ContentView::PlaylistDetail { detail, .. } = &mut state.view {
            detail.tracks.append(&mut new_tracks);
            detail.has_more = has_more;
            detail.loading_more = false;
        }
    }
}

/// Represents a selected item for action handling
#[derive(Clone, Debug)]
pub enum SelectedItem {
    Track { uri: String, name: String },
    Album { id: String, name: String },
    Artist { id: String, name: String },
    Playlist { id: String, name: String },
    /// A track within a playlist context (for playing from that track)
    PlaylistTrack { playlist_uri: String, track_uri: String, name: String },
    /// A track within an album context (for playing from that track)
    AlbumTrack { album_uri: String, track_uri: String, name: String },
}
