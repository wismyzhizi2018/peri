//! jemalloc allocator tuning for high-churn workloads.
//!
//! Two-phase configuration:
//! 1. `init_malloc_conf()` — sets `MALLOC_CONF` env var BEFORE jemalloc init.
//!    This is the only reliable way to enable `background_thread`.
//! 2. `configure_jemalloc()` — runtime mallctl writes as fallback/diagnostics.
//!
//! jemalloc reads `MALLOC_CONF` once at process startup (during the first
//! allocation). `background_thread` cannot be enabled via runtime `raw::write`
//! once arenas have been created (by tokio threads), so the env var approach
//! is essential.
//!
//! Configuration applied:
//! - `dirty_decay_ms: 200` — purge freed arena pages after 200ms (default: 1000ms+)
//! - `background_thread: true` — enable background purge thread (default: disabled)
//! - `lg_tcache_max: 16` — limit thread cache to objects ≤64KB (default: unlimited)

/// Set `MALLOC_CONF` environment variable before jemalloc initializes.
///
/// Call this at the very first line of `main()`, before any allocation.
/// jemalloc reads `MALLOC_CONF` during its one-time init (triggered by the
/// first allocation through `#[global_allocator]`). If the env var is already
/// set (e.g. by the user externally), it is not overwritten.
// Clippy: dead_code in lib targets; used by bin target main.rs.
#[allow(dead_code)]
#[cfg(not(target_os = "windows"))]
pub fn init_malloc_conf() {
    if std::env::var("MALLOC_CONF").is_ok() {
        return;
    }
    std::env::set_var(
        "MALLOC_CONF",
        "dirty_decay_ms:200,background_thread:true,lg_tcache_max:16",
    );
}

#[cfg(target_os = "windows")]
pub fn init_malloc_conf() {
    // jemalloc not used on Windows (system allocator instead)
}

/// Configure jemalloc for aggressive memory reclamation via runtime mallctl.
///
/// This is a best-effort fallback that applies settings at runtime.
/// `background_thread` may not take effect if arenas already exist;
/// use `init_malloc_conf()` for reliable configuration.
// Called from main.rs (bin target) via peri_tui::jemalloc_config::configure_jemalloc().
// Clippy's dead_code lint fires on lib targets even when used by the bin target.
#[allow(dead_code)]
#[cfg(not(target_os = "windows"))]
pub fn configure_jemalloc() {
    use tracing::{debug, warn};

    // Advance epoch to ensure stats are fresh
    let _ = tikv_jemalloc_ctl::epoch::advance();

    // 1. dirty_decay_ms — time before freed dirty pages are purged
    //    Default is 10000ms on many builds; we set 200ms for aggressive reclamation.
    //    Lower values increase CPU overhead from madvise syscalls but prevent
    //    the observed ~27MB dirty extent accumulation per turn.
    match unsafe { tikv_jemalloc_ctl::raw::write(b"arenas.dirty_decay_ms\0", 200i64) } {
        Ok(()) => debug!("jemalloc: arenas.dirty_decay_ms = 200"),
        Err(e) => warn!("jemalloc: failed to set dirty_decay_ms: {}", e),
    }

    // 2. background_thread — enables a background thread per arena that
    //    proactively purges dirty pages. Without this, purge only happens
    //    during foreground allocations (the "lazy" purge path), which can't
    //    keep up with our churn rate.
    match unsafe { tikv_jemalloc_ctl::raw::write(b"background_thread\0", true) } {
        Ok(()) => debug!("jemalloc: background_thread = true"),
        Err(e) => warn!("jemalloc: failed to enable background_thread: {}", e),
    }

    // 3. lg_tcache_max — log2 of max cached allocation size in thread caches.
    //    Default is ~23 (8MB), which means large allocations linger in tcache.
    //    Setting to 16 (64KB) limits tcache to small objects, reducing the
    //    5-7MB tcache_bytes overhead observed in heapdumps.
    match unsafe { tikv_jemalloc_ctl::raw::write(b"arenas.lg_tcache_max\0", 16usize) } {
        Ok(()) => debug!("jemalloc: arenas.lg_tcache_max = 16 (64KB)"),
        Err(e) => warn!("jemalloc: failed to set lg_tcache_max: {}", e),
    }
}

#[cfg(target_os = "windows")]
pub fn configure_jemalloc() {
    // jemalloc not used on Windows (system allocator instead)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_configure_jemalloc_does_not_panic() {
        configure_jemalloc();
        configure_jemalloc();
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_dirty_decay_ms_is_set() {
        configure_jemalloc();
        let _ = tikv_jemalloc_ctl::epoch::advance();
        let val: i64 = unsafe { tikv_jemalloc_ctl::raw::read(b"arenas.dirty_decay_ms\0") }
            .expect("should read dirty_decay_ms");
        assert_eq!(val, 200, "dirty_decay_ms should be 200ms after configure");
    }

    // Note: init_malloc_conf tests modify process-global env vars.
    // They run in a single-threaded context via #[serial] if needed,
    // but we use remove_var at start/end to be self-contained.
    #[test]
    fn test_init_malloc_conf_sets_env() {
        std::env::remove_var("MALLOC_CONF");
        init_malloc_conf();
        let val = std::env::var("MALLOC_CONF").expect("MALLOC_CONF should be set");
        assert!(
            val.contains("background_thread:true"),
            "MALLOC_CONF should contain background_thread:true, got: {}",
            val
        );
        assert!(
            val.contains("dirty_decay_ms:200"),
            "MALLOC_CONF should contain dirty_decay_ms:200, got: {}",
            val
        );
        assert!(
            val.contains("lg_tcache_max:16"),
            "MALLOC_CONF should contain lg_tcache_max:16, got: {}",
            val
        );
        std::env::remove_var("MALLOC_CONF");
    }

    #[test]
    fn test_init_malloc_conf_respects_existing() {
        std::env::remove_var("MALLOC_CONF");
        std::env::set_var("MALLOC_CONF", "custom:true");
        init_malloc_conf();
        let val = std::env::var("MALLOC_CONF").expect("MALLOC_CONF should be set");
        assert_eq!(
            val, "custom:true",
            "Should not overwrite user-set MALLOC_CONF"
        );
        std::env::remove_var("MALLOC_CONF");
    }
}
