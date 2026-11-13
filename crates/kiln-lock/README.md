# kiln-lock

KilnLock: lock/unlock state machine, argon2id passcode (0600 file), lockout schedule, biometric hook trait, auto-lock timer from screen-off.

**Status:** Implemented. Enforcement is the compositor's.

## TODO(feature)

- fprintd / face-unlock Biometric implementation
- Emergency-call-without-unlock policy
- Wipe-after-N-failures option
- Read auto-lock seconds from flow-settings (`lock.auto_lock_s`)
