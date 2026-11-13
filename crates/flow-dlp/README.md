# flow-dlp

FlowDLP: yt-dlp style download service. Queue and state machine (queued/downloading/paused/done/failed/cancelled, retry with backoff, resume via part files), Engine trait, yt-dlp engine (spawn, `--progress-template`, `--dump-json`, allowlisted args, optional bubblewrap), mock engine.

**Status:** Tests use the mock engine only; no download was performed and nothing is fetched unless a client calls `dlp.add`. LEGAL: you are responsible for content rights and site terms of service. Meaning of FlowDLP confirmed by the user as a yt-dlp implementation.

## TODO(feature)

- Native Rust extractor path (first targets: generic direct-file and HLS/DASH manifests) registered before the yt-dlp fallback
- Format selection UI data (list of formats with sizes)
- Playlist item selection and per-item progress
- Cookie import UI hook, auth hooks
- Post-processing (merge/convert) without yt-dlp
- Download directory per category, bandwidth limits, schedule on Wi-Fi only
