Spotify client written in rust

* Ratatui for TUI
* rspotify for spotify actions
* librespot for playback

MVP Version:

* Working audio playback - thread with librespot
* Thread with UI
* Single authentication request
    The application should request credentials in a single step,
        then use them to authenticate with Spotify both for rspotify and librespot.
        Librespot supports saving credentials in a blob, so we can store them locally.
        We should reuse them for rspotify as well. Or vice versa.
    We should be able to use sessions and require re-authentication only when necessary.
* Basic playback controls - play, pause, next, previous
* Display current track info


Future features:

* Search for tracks, albums, artists ✅
* Browse playlists, albums, artists ✅
* Track - play or add to queue ✅
* Album, Artist - play ✅
* Browse user's library ✅
* View queue ✅ 
* Remove from queue ✅ (via auto-skip: removed tracks are added to skip list and automatically skipped when they try to play; skip list clears when user plays new content)
* Volume control ✅
* Repeat, shuffle ✅
* Keyboard shortcuts for common actions ✅
* Homepage with recommended playlists, new releases, etc. ⚠️
* Select different devices for playback ✅
* Modifying playlists will be tough so: ❌
     Allow export of playlist to json and import from json
     This way you can manage your playlists outside of the app
     Should think of a way to get song ids


space - play/pause
p - previous
n - next
q - quit
- - volume down
+/= - volume up
s - shuffle
r - repeat
d - device select
h - help
y - settings
g - focus search
l - focus playlists
x - like
u - show queue
k - add to queue 
Delete - remove from queue


┌ Search ────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                                                                                 │ 🔉 librespot         │
└────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
┌ Library ───────────────────┐ ┌ Made for you ───────────────────────────────────────────────────────────────────────────┐
│ Made for you               │ │ #        Title                              Artist                             Duration │
│ Recently played            │ │ 1    💚  Am I really Going To Die           White Lies                             3:01 │
│ Liked songs                │ │ 2    💚  Ruby                               Kaiser Chiefs                          3:24 │
│ Albums                     │ │ 3                                                                                       │
│ Artists                    │ │ 4                                                                                       │
└────────────────────────────┘ │ 5                                                                                       │
┌ Playlists ─────────────────┐ │ 6                                                                                       │
│ Playlist Example 1         │ │ 7                                                                                       │
│ Playlist Example 2         │ │ 8                                                                                       │
│ Playlist Example 3         │ │                                                                                         │
│ Playlist Example 4         │ │                                                                                         │
│                            │ │                                                                                         │
│                            │ │                                                                                         │
│                            │ │                                                                                         │
│                            │ │                                                                                         │
│                            │ │                                                                                         │
│                            │ │                                                                                         │
│                            │ │                                                                                         │
│                            │ │                                                                                         │
│                            │ │                                                                                         │
│                            │ │                                                                                         │
└────────────────────────────┘ └─────────────────────────────────────────────────────────────────────────────────────────┘
┌ ▶ Track Name | Artist ( Album Name )                                          Shuffle: On | Repeat: Off | Volume: 98% ─┐
│                                                      0:00 / 0:00                                                       │
└────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘

