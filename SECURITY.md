# Security

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub: **Security** tab > **Report a vulnerability**. Do not open a public issue. You will get an answer as soon as possible, and a fix will be released before the details are made public.

## Verifying a download

Release files are built by the [Release workflow](.github/workflows/release.yml) on GitHub-hosted runners, from the tagged source of this repository. Nothing is built or uploaded by hand.

1. **Provenance**: each file carries a signed build attestation (Sigstore). With the GitHub CLI:

   ```
   gh attestation verify Rezo-0.2.0-windows-x64-setup.exe --repo theappstore-cell/Rezo
   ```

   The command checks that the file was produced by this repository's workflow, at a given commit.

2. **Checksum**: compare the file's SHA-256 with `SHA256SUMS.txt` from the same release.

   ```
   Get-FileHash .\Rezo-0.2.0-windows-x64-setup.exe -Algorithm SHA256   # Windows
   shasum -a 256 Rezo-0.2.0-macos-universal.dmg                         # macOS
   ```

3. **Dependencies**: `rezo-engine.cdx.json` and `rezo-recv.cdx.json` list every Rust dependency (CycloneDX SBOM).

The binaries are not code-signed yet. The workflow signs them automatically once a certificate is configured (`WINDOWS_CERTIFICATE` for Windows, `APPLE_*` secrets for macOS signing and notarization).

## What Rezo does on your machine

- No telemetry, no account. Besides devices on your local network, Rezo contacts one address: at start-up and every 6 hours it asks the GitHub API (`api.github.com`) for this repository's latest release, sending nothing but its version in the User-Agent. Turn it off in Settings > Updates.
- Updates are installed only when you click: the portable executable of the new release is downloaded over HTTPS into Rezo's own folder, checked against the release's `SHA256SUMS.txt` (the file is deleted if it does not match), then swapped with the running one and restarted. Where Rezo's folder is not writable (an all-users install), Rezo opens the release page instead.
- The Windows installer lets you choose between your account only (no administrator rights) and all users (administrator rights, also adds a Windows Firewall rule limited to private networks). A silent install (`/S`) run as administrator installs for all users. The portable executable installs nothing.
- Uninstalling removes the program, its shortcuts, its firewall rule and its login entry; your settings in `%APPDATA%\Rezo` are kept unless you tick "Delete the application data".
- The HTTP server that feeds speakers and browsers requires a random per-install key in every URL; other devices on the network get a 404 without it.
- The interface runs with a strict Content Security Policy and only three Tauri permissions (listen to events, stop listening, drag the window).
- When no speaker is enabled, no audio stream, real-time thread or MIDI port is held.

## Supply chain

- Third-party GitHub Actions are pinned to full commit SHAs.
- Lockfiles (`Cargo.lock`, `package-lock.json`) are committed and builds use `--locked` / `npm ci`.
- `cargo audit` and `npm audit` run on every push, CodeQL scans the code weekly, Dependabot proposes dependency updates.
