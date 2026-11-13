# kiln-dm

KilnDM: session catalogue (wayland-sessions/xsessions), autologin and preselection policy, greeter state machine with throttling, read-only logind session list, handover request record.

**Status:** Unprivileged logic only. `dm.login` returns `unavailable` because the PAM backend is not implemented.

## TODO(feature)

- PAM authentication backend (Auth trait) running as root
- Session launcher: setuid, logind session, XDG env, start FlowDE/Glaze
- Greeter UI protocol over a root-owned socket
- Real FlowDE/Glaze handover (switch or second session)
- Lock-screen unlock via PAM
