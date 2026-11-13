# flow-power

Battery (UPower, sysfs fallback), AC, thermal bands with hysteresis, backlight read, brightness set via logind, power profiles (power-profiles-daemon), low-power mode (auto/on/off), idle escalation (dim, screen off, suspend) and suspend through logind.

**Status:** Implemented against real buses/sysfs; none of it could be exercised against a battery in the VM.

## TODO(feature)

- ext-idle-notify-v1 client for real idle detection (today the shell calls `power.activity`)
- UPower and logind signals instead of polling
- Charge-limit and battery health
- Auto-brightness application (consume flow-sensors suggestions)
