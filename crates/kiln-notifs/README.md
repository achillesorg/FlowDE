# kiln-notifs

KilnNotifs: `org.freedesktop.Notifications` server (Notify, CloseNotification, GetCapabilities, GetServerInformation, NotificationClosed), persistence, DND (off/on/for N minutes, critical passes), grouping by app, history API over the socket.

**Status:** Implemented. No UI: banners are the shell's job (`notify.changed` events).

## TODO(feature)

- ActionInvoked signal (needs a `notify.invoke` call from the shell)
- Images/icons hints, markup sanitising for the shell
- Per-app notification permissions (join with flow-apps perms)
- Inline reply, progress hints
