# flow-clock

Alarms (weekday masks, snooze), timers (pause/resume), stopwatch (laps), fire-on-resume logic, wake timer via `systemd-run --user`.

**Status:** Implemented; real suspend/wake was not exercised.

## TODO(feature)

- RTC wake alarm fallback (`/sys/class/rtc/*/wakealarm`, needs privilege)
- Logind PrepareForSleep handling
- World clocks / time zones
- Alarm sound selection and gradual volume
