use std::sync::Arc;
use anyhow::Result;
use std::time::Instant;
use tokio::sync::Mutex;
use rspotify::{
    model::{CurrentPlaybackContext, PlayableItem, SearchType, Market, AlbumId, PlaylistId, ArtistId},
    prelude::*,
    AuthCodeSpotify,
};

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
    device_name: Option<String>,
}

impl SpotifyClient {
    pub fn new(client: AuthCodeSpotify, device_name: Option<String>) -> Self {
        Self {
            client: Arc::new(client),
            device_name,
        }
    }

    pub async fn get_current_playback(&self) -> Result<Option<CurrentPlaybackContext>> {
        Ok(self.client.current_playback(None, None::<Vec<_>>).await?)
    }

    async fn get_device_id(&self) -> Option<String> {
        if let Ok(devices) = self.client.device().await {
            // First try to find our librespot device
            if let Some(ref name) = self.device_name {
                if let Some(device) = devices.iter().find(|d| d.name == *name) {
                    return device.id.clone();
                }
            }
            // Fall back to active device
            devices.into_iter().find(|d| d.is_active).and_then(|d| d.id)
        } else {
            None
        }
    }

    pub async fn play(&self) -> Result<()> {
        let device_id = self.get_device_id().await;
        self.client
            .resume_playback(device_id.as_deref(), None)
            .await?;
        Ok(())
    }

    pub async fn pause(&self) -> Result<()> {
        let device_id = self.get_device_id().await;
        self.client.pause_playback(device_id.as_deref()).await?;
        Ok(())
    }

    pub async fn next_track(&self) -> Result<()> {
        let device_id = self.get_device_id().await;
        self.client.next_track(device_id.as_deref()).await?;
        Ok(())
    }

    pub async fn previous_track(&self) -> Result<()> {
        let device_id = self.get_device_id().await;
        self.client.previous_track(device_id.as_deref()).await?;
        Ok(())
    }

    pub async fn set_shuffle(&self, state: bool) -> Result<()> {
        let device_id = self.get_device_id().await;
        self.client.shuffle(state, device_id.as_deref()).await?;
        Ok(())
    }

    pub async fn set_repeat(&self, state: RepeatState) -> Result<()> {
        let device_id = self.get_device_id().await;
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
        self.client.volume(volume, device_id.as_deref()).await?;
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

    /// Get playlist details with tracks
    pub async fn get_playlist(&self, playlist_id: &str) -> Result<PlaylistDetail> {
        use rspotify::prelude::Id;

        let id = PlaylistId::from_id(playlist_id)?;
        let playlist = self.client.playlist(id.clone(), None, None).await?;

        let mut tracks = Vec::new();

        // Playlist tracks are included in the full playlist response
        for item in playlist.tracks.items.iter() {
            if let Some(PlayableItem::Track(track)) = &item.track {
                let track_id = track.id.as_ref().map(|id| id.id().to_string()).unwrap_or_default();
                tracks.push(SearchTrack {
                    uri: format!("spotify:track:{}", track_id),
                    id: track_id,
                    name: track.name.clone(),
                    artist: track.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
                    album: track.album.name.clone(),
                    duration_ms: track.duration.num_milliseconds() as u32,
                });
            }
        }

        Ok(PlaylistDetail {
            id: playlist_id.to_string(),
            uri: format!("spotify:playlist:{}", playlist_id),
            name: playlist.name,
            owner: playlist.owner.display_name.clone().unwrap_or_else(|| playlist.owner.id.to_string()),
            description: playlist.description.clone(),
            tracks,
        })
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

    /// Get user's liked songs (saved tracks)
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
                }
            })
            .collect();

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
}

impl Default for TrackMetadata {
    fn default() -> Self {
        Self {
            name: "No track playing".to_string(),
            artist: String::new(),
            album: String::new(),
            duration_ms: 0,
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

                    Self {
                        name: track.name.clone(),
                        artist,
                        album: track.album.name.clone(),
                        duration_ms: track.duration.num_milliseconds() as u32,
                    }
                }
                PlayableItem::Episode(episode) => Self {
                    name: episode.name.clone(),
                    artist: episode.show.name.clone(),
                    album: "Podcast".to_string(),
                    duration_ms: episode.duration.num_milliseconds() as u32,
                },
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
