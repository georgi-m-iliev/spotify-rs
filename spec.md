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