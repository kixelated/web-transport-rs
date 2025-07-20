# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.7.3](https://github.com/kixelated/web-transport-rs/compare/web-transport-quinn-v0.7.2...web-transport-quinn-v0.7.3) - 2025-07-20

### Other

- Re-export the http crate.

## [0.7.2](https://github.com/kixelated/web-transport-rs/compare/web-transport-quinn-v0.7.1...web-transport-quinn-v0.7.2) - 2025-06-02

### Fixed

- fix connecting to ipv6 using quinn backend ([#82](https://github.com/kixelated/web-transport-rs/pull/82))

## [0.7.1](https://github.com/kixelated/web-transport-rs/compare/web-transport-quinn-v0.7.0...web-transport-quinn-v0.7.1) - 2025-05-21

### Other

- Fully take ownership of the Url, not a ref. ([#80](https://github.com/kixelated/web-transport-rs/pull/80))

## [0.6.1](https://github.com/kixelated/web-transport-rs/compare/web-transport-quinn-v0.6.0...web-transport-quinn-v0.6.1) - 2025-05-21

### Other

- Add a required `url` to Session ([#75](https://github.com/kixelated/web-transport-rs/pull/75))

## [0.6.0](https://github.com/kixelated/web-transport-rs/compare/web-transport-quinn-v0.5.1...web-transport-quinn-v0.6.0) - 2025-05-15

### Other

- Add (generic) support for learning when a stream is closed. ([#73](https://github.com/kixelated/web-transport-rs/pull/73))

## [0.5.1](https://github.com/kixelated/web-transport-rs/compare/web-transport-quinn-v0.5.0...web-transport-quinn-v0.5.1) - 2025-03-26

### Fixed

- completely remove aws-lc when feature is off ([#69](https://github.com/kixelated/web-transport-rs/pull/69))

### Other

- Added Ring feature flag ([#68](https://github.com/kixelated/web-transport-rs/pull/68))
- Adding with_unreliable shim functions to wasm/quinn ClientBuilders for easier generic use ([#64](https://github.com/kixelated/web-transport-rs/pull/64))

## [0.5.0](https://github.com/kixelated/web-transport-rs/compare/web-transport-quinn-v0.4.1...web-transport-quinn-v0.5.0) - 2025-01-26

### Other

- Revamp client/server building. ([#60](https://github.com/kixelated/web-transport-rs/pull/60))

## [0.4.1](https://github.com/kixelated/web-transport-rs/compare/web-transport-quinn-v0.4.0...web-transport-quinn-v0.4.1) - 2025-01-15

### Other

- Switch to aws_lc_rs ([#58](https://github.com/kixelated/web-transport-rs/pull/58))
- Bump some deps. ([#55](https://github.com/kixelated/web-transport-rs/pull/55))
- Clippy fixes. ([#53](https://github.com/kixelated/web-transport-rs/pull/53))

## [0.4.0](https://github.com/kixelated/web-transport-rs/compare/web-transport-quinn-v0.3.4...web-transport-quinn-v0.4.0) - 2024-12-03

### Other

- Make a `Client` class to make configuration easier. ([#50](https://github.com/kixelated/web-transport-rs/pull/50))

## [0.3.4](https://github.com/kixelated/web-transport-rs/compare/web-transport-quinn-v0.3.3...web-transport-quinn-v0.3.4) - 2024-10-26

### Other

- Derive PartialEq for Session. ([#45](https://github.com/kixelated/web-transport-rs/pull/45))

## [0.3.3](https://github.com/kixelated/web-transport-rs/compare/web-transport-quinn-v0.3.2...web-transport-quinn-v0.3.3) - 2024-09-03

### Other
- Some more documentation. ([#42](https://github.com/kixelated/web-transport-rs/pull/42))

## [0.3.2](https://github.com/kixelated/web-transport-rs/compare/web-transport-quinn-v0.3.1...web-transport-quinn-v0.3.2) - 2024-08-15

### Other
- Some more documentation. ([#34](https://github.com/kixelated/web-transport-rs/pull/34))
