//! Serde helper for [`std::time::Duration`] — re-exported from [`limit3r`].

pub use limit3r::duration_serde::*;

/// Serde helper for `Option<Duration>` as optional fractional seconds.
pub mod option {
    pub use limit3r::duration_serde::option::*;
}
