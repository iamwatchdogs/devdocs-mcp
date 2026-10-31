# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/iamwatchdogs/devdocs-mcp/releases/tag/v0.1.0) - 2026-10-31

### Added

- *(cli)* register the command surface and the exit-code edge

### Fixed

- *(devdocs)* drop the unused testkit dev-dependency ([#30](https://github.com/iamwatchdogs/devdocs-mcp/pull/30))
- *(devdocs)* make the crate publishable with a path testkit dep ([#29](https://github.com/iamwatchdogs/devdocs-mcp/pull/29))
- *(model)* sharpen the config error edges
- share the stdout policy and sync the validate doc
- *(cli)* harden the command edge
- derive config path from the loaded configuration
- report the failing key path in the JSON error envelope

### Other

- *(cli)* cover transport, missing config, and URL validation
- *(cli)* quote windows paths as toml literal strings
- *(cli)* record why the local-failure codes stay
- *(cli)* drop the unused tracing dependency
- *(cli)* cover exit codes, precedence, and config end to end
- align agent instruction layer with scaffold reality
- fix workspace manifest target errors and unblock lint
- project scaffold
