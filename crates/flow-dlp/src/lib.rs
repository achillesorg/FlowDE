//! FlowDLP: a yt-dlp style download service.
//!
//! URL -> engine (extractor) -> format selection -> download/merge/post-process,
//! with queue, progress, resume, cookies hook, playlists, subtitles, metadata
//! and thumbnails. The first engine drives the real `yt-dlp` binary (spawned
//! shell-free, optionally under bubblewrap). A native Rust extractor is a
//! documented later goal behind the same [`Engine`] trait.
//!
//! LEGAL: users are responsible for having the right to download content and
//! for complying with each site's terms of service and local law. FlowDLP
//! ships no extractors for any site and bypasses no DRM or access control.

pub mod args;
pub mod engine;
pub mod queue;
