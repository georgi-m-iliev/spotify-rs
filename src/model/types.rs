//! Core type definitions for the application

use std::time::Instant;

/// Which section of the UI is currently active/focused
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

/// An item in the Library section
#[derive(Clone, Debug)]
pub struct LibraryItem {
    pub name: String,
}

/// Information about a Spotify playback device
#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub is_active: bool,
}

/// A user's playlist (for sidebar display)
#[derive(Clone, Debug)]
pub struct PlaylistItem {
    pub id: String,
    pub name: String,
}

/// Repeat mode state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepeatState {
    Off,
    All,
    One,
}

/// Which section of search results is selected
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

/// Represents a selected item for action handling
#[derive(Clone, Debug)]
pub enum SelectedItem {
    Track { uri: String },
    Album { id: String },
    Artist { id: String },
    Playlist { id: String },
    PlaylistTrack { playlist_uri: String, track_uri: String },
    AlbumTrack { album_uri: String, track_uri: String },
}

/// UI state for the application
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
    pub show_device_picker: bool,
    pub available_devices: Vec<DeviceInfo>,
    pub device_selected: usize,
    pub show_help_popup: bool,
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
            show_help_popup: false,
        }
    }
}
