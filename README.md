# spotify-rs

A textual Spotify client written in Rust.

## Core Dependencies

- ratatui
- rspotify
- librespot
- tokio
- serde_json

## Features

- Browse and search for tracks, albums, artists and playlists.
- Audio playback with play, pause, previous, next, random, shuffle and volume control.
- Display track information including title, artist, album, and duration.
- Browse user's playlists and saved tracks.
- Queue management with the ability to add tracks to the playback queue.
- Support for OAuth2 authentication with Spotify and caching of session.
- Responsive terminal user interface.

## Limitations

Due to limitations in the Spotify Web API and librespot, the following features are not available and couldn't be implemented:
- Removing tracks from the playback queue is not supported, as neither the Spotify Web API nor librespot expose this functionality.
- Playlists and mixes created by Spotify cannot be accessed via the API and can't be played

The first limitation was implemented by tracking the removed tracks and skipping them.

## Future Work

- Implement playlist management via JSON import/export.
- Create a settings menu for user preferences.
- Optimize performance and resource usage.
- Explore additional features based on user feedback.

## Installation

To install spotify-rs, ensure you have Rust and Cargo installed. Then clone the repository and build the project:

```bash
cargo build --release
```

You can find the compiled binary in the `target/release` directory.

## Usage

To run spotify-rs, execute the following command in your terminal:

```bash
cargo run --release
```

or run the compiled binary directly:

```bash
./target/release/spotify-rs
```

```shell
./target/release/spotify-rs.exe
```

## Contributing

Contributions are welcome! Please fork the repository and create a pull request with your changes.
Make sure to follow the existing code style and include tests for any new features.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Thanks to the developers of the libraries used in this project.
- Inspiration from other Spotify clients.