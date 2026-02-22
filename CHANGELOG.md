# Changelog

## [Unreleased]

### Changed

- Docker image now uses Google's ["distroless" base image](https://github.com/GoogleContainerTools/distroless).

## [0.4.0] - 2026-02-16

### Added
- cupcake is now available as a Docker image.
- Options can now be specified as environment variables.

### Changed
- Guest logins are retried when throttled by the server.
- cupcake terminates gracefully on `SIGTERM` signals.
- Slight improves to disconnect handling.

## [0.3.0] - 2025-11-30

### Added
- Option to automatically rotate the log file every N hours.

### Fixed
- Quotes in chat messages are now escaped for the CSV parser.

### Changed
- Switched the thread data sharing mechanism to an alternative, faster implementation (crossfire).
  - The old tokio implementation can still be selected at compile time with feature flags.
- Improved built-in help.

## [0.2.0] - 2025-10-21

### Changed
- Writes to the chat log file are now buffered. The buffer is fixed at 8 KiB.
- Server whispers, like voteskip results, are no longer included in the chat log.
- Minor changes to logging.
- Project updated to Rust 2024 Edition.

## [0.1.0] - 2025-10-17

Initial release.

[Unreleased]: https://github.com/Hamuko/cupcake/compare/0.4.0...HEAD
[0.4.0]: https://github.com/Hamuko/cupcake/compare/0.3.0...0.4.0
[0.3.0]: https://github.com/Hamuko/cupcake/compare/0.2.0...0.3.0
[0.2.0]: https://github.com/Hamuko/cupcake/compare/0.1.0...0.2.0
[0.1.0]: https://github.com/Hamuko/cupcake/releases/tag/0.1.0
