# kiln-desktop-mode

KilnDesktop backend: external-connector detection (DRM sysfs), debounce, policy auto/ask/never, start/stop the Glaze session command, `desktop.input_redirect` events.

**Status:** Controller and polling implemented and tested; launching uses the configured command (`glaze-session --output {output}` by default).

## TODO(feature)

- udev/netlink hotplug instead of polling
- DP-alt/dock identification (USB-C PD, dock USB ids)
- Real input redirection with the compositor (pointer/keyboard focus to the external output)
- Window migration between phone shell and desktop session
- Per-dock remembered layout/resolution
