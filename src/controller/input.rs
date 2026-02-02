//! Key event handling

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::model::ActiveSection;
use super::AppController;

impl AppController {
    pub async fn handle_key_event(&self, key: KeyEvent) -> Result<()> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        let model = self.model.lock().await;

        // Handle error message first (blocks all other interactions)
        if model.has_error().await {
            return match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    model.clear_error().await;
                    Ok(())
                }
                _ => Ok(()),
            }
        }

        // Handle help popup
        if model.is_help_popup_open().await {
            return match key.code {
                KeyCode::Esc | KeyCode::Char('h') | KeyCode::Char('H') => {
                    model.hide_help_popup().await;
                    Ok(())
                }
                _ => Ok(()),
            }
        }

        // Handle device picker modal
        if model.is_device_picker_open().await {
            return match key.code {
                KeyCode::Up => {
                    model.device_picker_move_up().await;
                    Ok(())
                }
                KeyCode::Down => {
                    model.device_picker_move_down().await;
                    Ok(())
                }
                KeyCode::Enter => {
                    if let Some(device) = model.get_selected_device().await {
                        let local_device_name = model.get_local_device_name().await;
                        model.hide_device_picker().await;
                        drop(model);
                        self.select_device(&device, &local_device_name).await;
                    }
                    Ok(())
                }
                KeyCode::Esc | KeyCode::Char('d') | KeyCode::Char('D') => {
                    model.hide_device_picker().await;
                    Ok(())
                }
                _ => Ok(()),
            }
        }

        let ui_state = model.get_ui_state().await;

        // Handle search input when in search section
        if ui_state.active_section == ActiveSection::Search {
            match key.code {
                KeyCode::Tab => {
                    if key.modifiers.contains(KeyModifiers::SHIFT) {
                        model.cycle_section_backward().await;
                    } else {
                        model.cycle_section_forward().await;
                    }
                    return Ok(());
                }
                KeyCode::Enter => {
                    let query = ui_state.search_query.clone();
                    drop(model);
                    if !query.is_empty() {
                        self.perform_search(&query).await;
                    }
                    return Ok(());
                }
                KeyCode::Esc => {
                    model.update_search_query(String::new()).await;
                    return Ok(());
                }
                KeyCode::Backspace => {
                    model.backspace_search().await;
                    return Ok(());
                }
                KeyCode::Char(c) => {
                    // Q still quits even in search mode when Ctrl is pressed
                    if (c == 'q' || c == 'Q') && key.modifiers.contains(KeyModifiers::CONTROL) {
                        model.set_should_quit(true).await;
                        return Ok(());
                    }
                    model.append_to_search(c).await;
                    return Ok(());
                }
                _ => {}
            }
        }

        // Handle MainContent section navigation
        if ui_state.active_section == ActiveSection::MainContent {
            match key.code {
                KeyCode::Up => {
                    model.content_move_up().await;
                    return Ok(());
                }
                KeyCode::Down => {
                    model.content_move_down().await;
                    // Check if we need to load more playlist tracks (spawn in background)
                    if let Some((playlist_id, offset)) = model.should_load_more_playlist_tracks().await {
                        let controller = self.clone();
                        tokio::spawn(async move {
                            controller.load_more_playlist_tracks(&playlist_id, offset).await;
                        });
                    }
                    return Ok(());
                }
                KeyCode::Left => {
                    model.navigate_search_section(false).await;
                    return Ok(());
                }
                KeyCode::Right => {
                    model.navigate_search_section(true).await;
                    return Ok(());
                }
                KeyCode::Enter => {
                    let selected = model.get_selected_content_item().await;
                    drop(model);
                    if let Some(item) = selected {
                        self.handle_selected_item(item).await;
                    }
                    return Ok(());
                }
                KeyCode::Backspace | KeyCode::Esc => {
                    model.navigate_back().await;
                    return Ok(());
                }
                KeyCode::Char('x') | KeyCode::Char('X') => {
                    if let Some((track_id, _is_liked)) = model.get_selected_track_for_like().await {
                        drop(model);
                        self.toggle_liked_track(&track_id).await;
                    }
                    return Ok(());
                }
                KeyCode::Char('k') | KeyCode::Char('K') => {
                    if let Some(track_uri) = model.get_selected_track_uri().await {
                        drop(model);
                        self.add_track_to_queue(&track_uri).await;
                    }
                    return Ok(());
                }
                KeyCode::Delete => {
                    if let Some(index) = model.get_selected_queue_index().await {
                        if let Some(uri) = model.remove_from_queue_view(index).await {
                            model.add_to_queue_skip_list(uri).await;
                            tracing::info!("Track added to skip list (will be auto-skipped)");
                        }
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
        
        // Global keybindings
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                model.set_should_quit(true).await;
            }
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    model.cycle_section_backward().await;
                } else {
                    model.cycle_section_forward().await;
                }
            }
            KeyCode::BackTab => {
                model.cycle_section_backward().await;
            }
            KeyCode::Up => {
                model.move_selection_up().await;
            }
            KeyCode::Down => {
                model.move_selection_down().await;
            }
            KeyCode::Enter => {
                // Handle Enter based on active section
                let ui_state = model.get_ui_state().await;
                match ui_state.active_section {
                    ActiveSection::Library => {
                        // Open selected library item
                        let selected = ui_state.library_selected;
                        drop(model);
                        self.open_library_item(selected).await;
                        return Ok(());
                    }
                    ActiveSection::Playlists => {
                        // Open selected playlist
                        if let Some(playlist) = model.get_selected_playlist().await {
                            drop(model);
                            self.open_playlist(&playlist.id).await;
                            return Ok(());
                        }
                    }
                    _ => {}
                }
            }
            // Play/Pause toggle
            KeyCode::Char(' ') => {
                drop(model);
                self.toggle_playback().await;
            }
            // Next track
            KeyCode::Char('n') | KeyCode::Char('N') => {
                drop(model);
                self.next_track().await;
            }
            // Previous track
            KeyCode::Char('p') | KeyCode::Char('P') => {
                drop(model);
                self.previous_track().await;
            }
            // Toggle shuffle
            KeyCode::Char('s') | KeyCode::Char('S') => {
                drop(model);
                self.toggle_shuffle().await;
            }
            // Cycle repeat mode
            KeyCode::Char('r') | KeyCode::Char('R') => {
                drop(model);
                self.cycle_repeat().await;
            }
            // Volume up
            KeyCode::Char('+') | KeyCode::Char('=') => {
                drop(model);
                self.volume_up().await;
            }
            // Volume down
            KeyCode::Char('-') => {
                drop(model);
                self.volume_down().await;
            }
            // Open device picker
            KeyCode::Char('d') | KeyCode::Char('D') => {
                drop(model);
                self.open_device_picker().await;
            }
            // Focus search
            KeyCode::Char('g') | KeyCode::Char('G') => {
                model.set_active_section(ActiveSection::Search).await;
            }
            // Focus playlists
            KeyCode::Char('l') | KeyCode::Char('L') => {
                model.set_active_section(ActiveSection::Playlists).await;
            }
            // Show queue
            KeyCode::Char('u') | KeyCode::Char('U') => {
                drop(model);
                self.show_queue().await;
            }
            // Show help popup
            KeyCode::Char('h') | KeyCode::Char('H') => {
                model.show_help_popup().await;
            }
            _ => {}
        }
        Ok(())
    }
}
