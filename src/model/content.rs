//! Content view state and data structures for search results, playlists, albums, etc.

use super::types::{ArtistDetailSection, SearchResultSection};

/// A track from search results or playlist
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
        // Artists have higher priority for exact matches (e.g., searching "Coldplay")
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

        // Playlists have lower priority
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

/// Album detail view data
#[derive(Clone, Debug)]
pub struct AlbumDetail {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub year: String,
    pub tracks: Vec<SearchTrack>,
}

/// Playlist detail view data
#[derive(Clone, Debug)]
pub struct PlaylistDetail {
    pub id: String,
    pub uri: String,
    pub name: String,
    pub owner: String,
    pub tracks: Vec<SearchTrack>,
    pub total_tracks: u32,
    pub has_more: bool,
    pub loading_more: bool,
}

/// Artist detail view data
#[derive(Clone, Debug)]
pub struct ArtistDetail {
    pub name: String,
    pub genres: Vec<String>,
    pub top_tracks: Vec<SearchTrack>,
    pub albums: Vec<SearchAlbum>,
}

/// Represents the current view in the main content area
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
    /// Queue view - shows currently playing and upcoming tracks
    Queue {
        currently_playing: Option<SearchTrack>,
        queue: Vec<SearchTrack>,
        selected_index: usize,
    },
}

/// State for the main content area
#[derive(Clone, Debug, Default)]
pub struct ContentState {
    pub view: ContentView,
    pub navigation_stack: Vec<ContentView>,
    pub is_loading: bool,
}
