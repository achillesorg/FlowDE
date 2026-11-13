# flow-store

App catalogue backend: apt-cache search, dpkg installed set, Flatpak search, install/remove job queue (pkexec apt-get, flatpak CLI) with validated identifiers.

**Status:** Implemented; installs were not run in tests (they need root/polkit and network).

## TODO(feature)

- PackageKit backend (no pkexec for apt)
- Update management, appstream metadata, screenshots
- Flathub remote provisioning
- Ratings/featured lists
