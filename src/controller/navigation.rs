//! Navigation-related controller methods (library, playlists, search)

use crate::model::ActiveSection;
use super::AppController;

pub const SEARCH_LIMIT: usize = 40;

impl AppController {
    pub async fn perform_search(&self, query: &str) {
        tracing::debug!(query, "Performing search");
        let model = self.model.lock().await;
        model.set_content_loading(true).await;

        if let Some(spotify) = &model.spotify {
            match spotify.search(query, SEARCH_LIMIT as u32).await {
                Ok(mut results) => {
                    tracing::info!(
                        query,
                        tracks = results.tracks.len(),
                        albums = results.albums.len(),
                        artists = results.artists.len(),
                        playlists = results.playlists.len(),
                        "Search completed successfully"
                    );
                    spotify.mark_tracks_liked(&mut results.tracks).await;
                    model.set_search_results(results).await;
                    // Switch to MainContent section to show results
                    let mut ui_state = model.ui_state.lock().await;
                    ui_state.active_section = ActiveSection::MainContent;
                }
                Err(e) => {
                    tracing::error!(query, error = %e, "Search failed");
                    model.set_content_loading(false).await;
                    let error_msg = Self::format_error(&e);
                    model.set_error(error_msg).await;
                }
            }
        }
    }

    pub async fn load_user_playlists(&self) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            match spotify.get_user_playlists(50).await {
                Ok(playlists) => {
                    model.set_playlists(playlists).await;
                }
                Err(e) => {
                    let error_msg = Self::format_error(&e);
                    model.set_error(error_msg).await;
                }
            }
        }
    }

    pub async fn open_playlist(&self, playlist_id: &str) {
        let model = self.model.lock().await;
        model.set_content_loading(true).await;

        if let Some(spotify) = &model.spotify {
            match spotify.get_playlist(playlist_id).await {
                Ok(mut detail) => {
                    spotify.mark_tracks_liked(&mut detail.tracks).await;
                    model.set_playlist_detail(detail).await;
                    // Switch to MainContent section to show playlist details
                    let mut ui_state = model.ui_state.lock().await;
                    ui_state.active_section = ActiveSection::MainContent;
                }
                Err(e) => {
                    model.set_content_loading(false).await;
                    let error_msg = Self::format_error(&e);
                    model.set_error(error_msg).await;
                }
            }
        }
    }

    pub async fn load_more_playlist_tracks(&self, playlist_id: &str, offset: usize) {
        tracing::debug!(playlist_id, offset, "Loading more playlist tracks");

        let model = self.model.lock().await;
        model.set_playlist_loading_more(true).await;

        if let Some(spotify) = &model.spotify {
            let spotify_clone = spotify.clone();
            let playlist_id = playlist_id.to_string();
            drop(model);

            match spotify_clone.get_more_playlist_tracks(&playlist_id, offset).await {
                Ok((mut tracks, _total_tracks, has_more)) => {
                    tracing::info!(
                        playlist_id,
                        loaded = tracks.len(),
                        has_more,
                        "Loaded more playlist tracks"
                    );
                    spotify_clone.mark_tracks_liked(&mut tracks).await;

                    let model = self.model.lock().await;
                    model.append_playlist_tracks(tracks, has_more).await;
                }
                Err(e) => {
                    tracing::error!(playlist_id, error = %e, "Failed to load more playlist tracks");
                    let model = self.model.lock().await;
                    model.set_playlist_loading_more(false).await;
                    let error_msg = Self::format_error(&e);
                    model.set_error(error_msg).await;
                }
            }
        }
    }

    pub async fn open_library_item(&self, index: usize) {
        let model = self.model.lock().await;
        model.set_content_loading(true).await;

        if let Some(spotify) = &model.spotify {
            let result = match index {
                0 => {
                    // Recently played
                    match spotify.get_recently_played(50).await {
                        Ok(mut tracks) => {
                            spotify.mark_tracks_liked(&mut tracks).await;
                            model.set_recently_played(tracks).await;
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                1 => {
                    // Liked songs (already marked as liked in get_liked_songs)
                    match spotify.get_liked_songs(100).await {
                        Ok(tracks) => {
                            model.set_liked_songs(tracks).await;
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                2 => {
                    // Albums
                    match spotify.get_saved_albums(50).await {
                        Ok(albums) => {
                            model.set_saved_albums(albums).await;
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                3 => {
                    // Artists
                    match spotify.get_followed_artists(50).await {
                        Ok(artists) => {
                            model.set_followed_artists(artists).await;
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                _ => {
                    model.set_content_loading(false).await;
                    return;
                }
            };

            if let Err(e) = result {
                model.set_content_loading(false).await;
                let error_msg = Self::format_error(&e);
                model.set_error(error_msg).await;
            } else {
                // Switch to MainContent section to show results
                let mut ui_state = model.ui_state.lock().await;
                ui_state.active_section = ActiveSection::MainContent;
            }
        }
    }

    pub async fn handle_selected_item(&self, item: crate::model::SelectedItem) {
        use crate::model::SelectedItem;
        
        match item {
            SelectedItem::Track { uri, .. } => {
                if !self.ensure_device_available().await {
                    return;
                }

                let model = self.model.lock().await;
                model.clear_queue_skip_list().await;

                if let Some(spotify) = &model.spotify {
                    let spotify_clone = spotify.clone();
                    let uri_clone = uri.clone();
                    drop(model);

                    let operation = move || {
                        let spotify = spotify_clone.clone();
                        let uri = uri_clone.clone();
                        async move { spotify.play_track(&uri).await }
                    };

                    if let Err(e) = self.with_backend_recovery(operation).await {
                        let model = self.model.lock().await;
                        let error_msg = Self::format_error(&e);
                        model.set_error(error_msg).await;
                    }
                }
            }
            SelectedItem::PlaylistTrack { playlist_uri, track_uri, .. } => {
                if !self.ensure_device_available().await {
                    return;
                }

                let model = self.model.lock().await;
                model.clear_queue_skip_list().await;

                if let Some(spotify) = &model.spotify {
                    let spotify_clone = spotify.clone();
                    let playlist_uri_clone = playlist_uri.clone();
                    let track_uri_clone = track_uri.clone();
                    drop(model);

                    let operation = move || {
                        let spotify = spotify_clone.clone();
                        let playlist_uri = playlist_uri_clone.clone();
                        let track_uri = track_uri_clone.clone();
                        async move { spotify.play_context_from_track_uri(&playlist_uri, &track_uri).await }
                    };

                    if let Err(e) = self.with_backend_recovery(operation).await {
                        let model = self.model.lock().await;
                        let error_msg = Self::format_error(&e);
                        model.set_error(error_msg).await;
                    }
                }
            }
            SelectedItem::AlbumTrack { album_uri, track_uri, .. } => {
                if !self.ensure_device_available().await {
                    return;
                }

                let model = self.model.lock().await;
                model.clear_queue_skip_list().await;

                if let Some(spotify) = &model.spotify {
                    let spotify_clone = spotify.clone();
                    let album_uri_clone = album_uri.clone();
                    let track_uri_clone = track_uri.clone();
                    drop(model);

                    let operation = move || {
                        let spotify = spotify_clone.clone();
                        let album_uri = album_uri_clone.clone();
                        let track_uri = track_uri_clone.clone();
                        async move { spotify.play_context_from_track_uri(&album_uri, &track_uri).await }
                    };

                    if let Err(e) = self.with_backend_recovery(operation).await {
                        let model = self.model.lock().await;
                        let error_msg = Self::format_error(&e);
                        model.set_error(error_msg).await;
                    }
                }
            }
            SelectedItem::Album { id, .. } => {
                // Open album detail
                let model = self.model.lock().await;
                model.set_content_loading(true).await;
                if let Some(spotify) = &model.spotify {
                    match spotify.get_album(&id).await {
                        Ok(mut detail) => {
                            spotify.mark_tracks_liked(&mut detail.tracks).await;
                            model.set_album_detail(detail).await;
                        }
                        Err(e) => {
                            model.set_content_loading(false).await;
                            let error_msg = Self::format_error(&e);
                            model.set_error(error_msg).await;
                        }
                    }
                }
            }
            SelectedItem::Artist { id, .. } => {
                // Open artist detail
                let model = self.model.lock().await;
                model.set_content_loading(true).await;
                if let Some(spotify) = &model.spotify {
                    match spotify.get_artist(&id).await {
                        Ok(mut detail) => {
                            spotify.mark_tracks_liked(&mut detail.top_tracks).await;
                            model.set_artist_detail(detail).await;
                        }
                        Err(e) => {
                            model.set_content_loading(false).await;
                            let error_msg = Self::format_error(&e);
                            model.set_error(error_msg).await;
                        }
                    }
                }
            }
            SelectedItem::Playlist { id, .. } => {
                // Open playlist detail
                let model = self.model.lock().await;
                model.set_content_loading(true).await;
                if let Some(spotify) = &model.spotify {
                    match spotify.get_playlist(&id).await {
                        Ok(mut detail) => {
                            spotify.mark_tracks_liked(&mut detail.tracks).await;
                            model.set_playlist_detail(detail).await;
                        }
                        Err(e) => {
                            model.set_content_loading(false).await;
                            let error_msg = Self::format_error(&e);
                            model.set_error(error_msg).await;
                        }
                    }
                }
            }
        }
    }
}
