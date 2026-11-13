# flow-calendar

Local calendar: ICS import (VEVENT, DTSTART/DTEND, RRULE daily/weekly/monthly/yearly with INTERVAL/COUNT/UNTIL/BYDAY, VALARM minutes), range queries, reminders.

**Status:** Implemented and tested.

## TODO(feature)

- CalDAV sync (fields href/etag/dirty are ready)
- TZID and DST-aware recurrence
- EXDATE/RECURRENCE-ID overrides, BYMONTHDAY/BYSETPOS
- ICS export
- Task (VTODO) support
