# Windows packaging

## Building installers

On Windows 10/11 with the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
(Microsoft C++ Build Tools, WebView2, Rust MSVC toolchain, Node.js 22+):

```powershell
npm ci
npm run tauri build
```

Outputs (configured in `src-tauri/tauri.conf.json` → `bundle.targets`):

| File | Location |
| --- | --- |
| MSI (WiX) | `target/release/bundle/msi/RADIAL_0.1.0_x64_en-US.msi` |
| NSIS setup | `target/release/bundle/nsis/RADIAL_0.1.0_x64-setup.exe` |
| Portable exe | `target/release/radial.exe` (needs WebView2 present) |

The NSIS installer installs per-user (no admin rights). On Windows 10 machines without WebView2
it downloads Microsoft's bootstrapper during install.

The `Windows build` job in `.github/workflows/ci.yml` builds both installers on `windows-latest`
and uploads them as workflow artifacts.

## Versioning

The version lives in three places that must match: `package.json`, `Cargo.toml`
(`[workspace.package]`) and `src-tauri/tauri.conf.json`.

## Code signing (not configured)

Installers are currently **unsigned**, so SmartScreen shows "Windows protected your PC" until
reputation builds. To sign:

1. Obtain an OV or EV code-signing certificate (or use Azure Trusted Signing).
2. Store it as CI secrets — never in the repository:
   - certificate thumbprint → `bundle.windows.certificateThumbprint` (local, cert in the user store), or
   - a custom `bundle.windows.signCommand` that calls `signtool` / `trusted-signing-cli`
     using credentials from environment variables.
3. Set `bundle.windows.timestampUrl` (e.g. `http://timestamp.digicert.com`).
4. Verify with `signtool verify /pa /v <installer>`.

See the [Tauri Windows signing guide](https://v2.tauri.app/distribute/sign/windows/).

## Optional engines

v0.1 has no external dependencies; the image engine (including libwebp) is compiled into the
executable. Future FFmpeg/LibreOffice support will detect user-installed engines at runtime and
will not be bundled by default (see `ROADMAP.md`).

## Verifying a build

1. Install, launch from the Start menu.
2. Drop a PNG with transparency; convert to JPEG and WebP; check the outputs open in Photos.
3. Try a file name with non-Latin characters in a folder with spaces.
4. Uninstall; confirm `%APPDATA%\app.radial.desktop` (history/settings) is the only leftover.
