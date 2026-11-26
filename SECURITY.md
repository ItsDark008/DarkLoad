# Security policy

## Supported versions

Security fixes are released for the latest minor version of DarkLoad.

| Version | Supported |
| --- | --- |
| 1.x (latest) | Yes |
| Older | No |

## Reporting a vulnerability

Please report vulnerabilities privately through **GitHub Security Advisories** ("Report a vulnerability" on the repository's Security tab). Do not open public issues for security problems.

Include:

- the affected version and operating system,
- a description of the issue and its impact,
- steps or a proof of concept to reproduce it.

You should receive an acknowledgement within seven days. Confirmed issues are fixed in a patch release and credited in the release notes unless you prefer otherwise.

## Security design

- Tauri capabilities grant the interface only the operations it needs, and every backend command is explicitly allowed.
- A strict Content Security Policy blocks remote scripts and remote images.
- External tools are started directly with argument lists — never through a shell — and user input is validated before it reaches them.
- File names are sanitized (including Windows reserved names), output paths are validated, and finished files never silently overwrite existing ones unless the user chooses that policy.
- yt-dlp, FFmpeg and Deno are downloaded from their official GitHub releases and installed only after their published SHA-256 checksums match.
- Application updates are accepted only with a valid signature from the project's update key.
- TikTok and Instagram account connections are optional. Credentials are entered only on the platform's sign-in page, which opens in a webview without access to DarkLoad's commands.
- Session cookies stay in DarkLoad's local webview storage. DarkLoad passes them to yt-dlp in a temporary file for analysis or downloads from the matching platform, then removes the file. Disconnecting removes the platform's session cookies from the webview.
- An account connection uses the account's existing access. DRM, paywalls and private-content protections still apply.
- The TikTok guest fallback runs in an isolated webview without application commands. It accepts media URLs only from TikTok's CDN domains and uses a temporary guest cookie file when yt-dlp downloads the selected stream.
