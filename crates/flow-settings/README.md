# flow-settings

Typed settings store: TOML schema (built in plus drop-ins), validation (bool/int/float/string/enum, ranges, hex colours), persistence, change events on socket and `org.flowde.Settings1`, atomic import/export.

**Status:** Implemented and tested.

## TODO(feature)

- dconf/gsettings bridge for GTK apps
- Per-user profiles, enterprise locks
- Migrations for the values file when keys are renamed
