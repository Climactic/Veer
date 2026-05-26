# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.2] - 2026-05-26

### Added

- `csrf` feature: `CsrfLayer`, a standalone tower layer providing
  Inertia/axios-compatible CSRF protection via stateless HMAC-signed
  double-submit tokens, plus the framework-agnostic `CsrfTokens` core.
- `embed` feature: `EmbeddedAssets`, an axum service for serving build assets
  embedded in the binary (rust-embed / `include_dir` / map) — the single-binary
  deploy counterpart to `ServeDir`.

[Unreleased]: https://github.com/Climactic/Veer/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/Climactic/Veer/compare/v0.1.1...v0.1.2
