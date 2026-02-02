//! Model module - Application state and data types
//!
//! This module contains all the data structures and state management for the application.
//! It is organized into submodules by responsibility:
//!
//! - `types`: Core type definitions (enums, UI state, etc.)
//! - `playback`: Playback-related state (track metadata, timing, settings)
//! - `content`: Content view data (search results, playlists, albums, etc.)
//! - `cache`: Liked songs cache for fast lookup
//! - `spotify_client`: Spotify API client wrapper
//! - `app_model`: Main application model with state management methods

mod types;
mod playback;
mod content;
mod cache;
mod spotify_client;
mod app_model;

// Re-export all public types for convenient access
pub use types::{
    ActiveSection, DeviceInfo, RepeatState,
    SearchResultSection, ArtistDetailSection, SelectedItem, UiState,
};

pub use playback::{
    TrackMetadata, PlaybackInfo,
};

pub use content::{
    SearchTrack, SearchAlbum, SearchArtist, SearchPlaylist, SearchResults,
    AlbumDetail, PlaylistDetail, ArtistDetail, ContentView, ContentState,
};


pub use spotify_client::SpotifyClient;

pub use app_model::AppModel;
