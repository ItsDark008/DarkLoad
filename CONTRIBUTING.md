# Contributing to DarkLoad

Thank you for helping improve DarkLoad. This guide explains how to propose changes.

## Ground rules

- Keep DarkLoad focused: a fast, private, high-quality downloader for publicly accessible media.
- Contributions that bypass DRM, paywalls, private-content protections, authentication or other access controls will not be accepted.
- Do not add telemetry, analytics, tracking, advertising or mandatory accounts.
- Write code, comments, documentation and interface text in clear English.
- Wrap interface text in `t("…")` (or `tr()` for messages from the backend) and add the Italian translation to `src/i18n/it.ts` (or `src/i18n/it-backend.ts`). `pnpm test` fails when a string has no translation.

## Getting started

1. Read [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) and install the prerequisites.
2. Fork the repository and create a branch from `main`, for example `fix/queue-retry-label`.
3. Run the app with `pnpm tauri dev`.

## Before opening a pull request

Run every check that CI runs:

```powershell
pnpm typecheck
pnpm lint
pnpm test
cd src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

- Add or update tests for behavior you change. Backend logic should be covered by Rust unit tests; queue behavior can use the `fake_ytdlp` example.
- Keep pull requests focused on one change and describe the user-visible effect.
- Update `CHANGELOG.md` under **Unreleased** for user-facing changes.
- Include screenshots for interface changes.

## Commit messages

Use short, imperative subject lines that describe the change, for example `Fix retry label after manual retry`. Explain the reason in the body when it is not obvious.

## Reporting bugs and requesting features

Use the issue templates. For bugs, attach the report from **Settings → Dependencies → Diagnostics** and the exact steps to reproduce. Never include private links or personal data.

## Security issues

Please do not open public issues for vulnerabilities. Follow [SECURITY.md](SECURITY.md).

## License

By contributing you agree that your contributions are licensed under the MIT License.
