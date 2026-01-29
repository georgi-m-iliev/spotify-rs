# spotify-rs

A textual Spotify client written in Rust.

## Dependencies

- ratatui
- rspotify
- librespot
- tokio

## Features

## Limitations

Due to limitations in the Spotify Web API and librespot, the following features are not available and couldn't be implemented:
- Removing tracks from the playback queue is not supported, as neither the Spotify Web API nor librespot expose this functionality.
- Playlists and mixes created by Spotify cannot be accessed via the API and can't be played

The first limitation was implemented by tracking the removed tracks and skipping them.

## Installation

## Usage

## Contributing

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Thanks to the developers of the libraries used in this project.
- Inspiration from other Spotify clients.