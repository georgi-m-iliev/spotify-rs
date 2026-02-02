//! Cache for liked songs to enable fast lookup without API calls

use std::sync::Arc;
use std::collections::HashSet;
use anyhow::Result;
use tokio::sync::RwLock;

const LIKED_SONGS_CACHE_FILE: &str = ".cache/liked_songs.json";

/// Cache for liked song IDs to enable fast lookup without API calls
#[derive(Clone)]
pub struct LikedSongsCache {
    liked_ids: Arc<RwLock<HashSet<String>>>,
    loaded: Arc<RwLock<bool>>,
}

impl LikedSongsCache {
    pub fn new() -> Self {
        Self {
            liked_ids: Arc::new(RwLock::new(HashSet::new())),
            loaded: Arc::new(RwLock::new(false)),
        }
    }

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

    pub async fn save_to_disk(&self) -> Result<()> {
        use std::fs;
        use std::path::Path;

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

    pub async fn update(&self, track_ids: Vec<String>) {
        let mut liked_ids = self.liked_ids.write().await;
        *liked_ids = track_ids.into_iter().collect();
        let mut loaded = self.loaded.write().await;
        *loaded = true;
    }

    pub async fn is_liked(&self, track_id: &str) -> bool {
        let liked_ids = self.liked_ids.read().await;
        liked_ids.contains(track_id)
    }

    pub async fn add(&self, track_id: String) {
        let mut liked_ids = self.liked_ids.write().await;
        liked_ids.insert(track_id);
    }

    pub async fn remove(&self, track_id: &str) {
        let mut liked_ids = self.liked_ids.write().await;
        liked_ids.remove(track_id);
    }
}

impl Default for LikedSongsCache {
    fn default() -> Self {
        Self::new()
    }
}
