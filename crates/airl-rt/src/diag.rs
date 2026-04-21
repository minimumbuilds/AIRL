//! Env-gated runtime memory tracing. Enabled by AIRL_RT_TRACE=1.
//!
//! Hooks: `on_alloc()` in `RtValue::alloc`, `on_free()` in memory.rs when
//! rc reaches 0. On first call, spawns a background thread that prints
//! `[rt-trace] rss=X MiB alive=Y allocs=Z freed=W` every 2 seconds.

use std::sync::atomic::{AtomicU64, AtomicBool, Ordering::Relaxed};

static ALLOCS: AtomicU64 = AtomicU64::new(0);
static FREES: AtomicU64 = AtomicU64::new(0);
static TRACE_ENABLED: AtomicBool = AtomicBool::new(false);
static TRACE_INITIALIZED: AtomicBool = AtomicBool::new(false);

// Per-tag alive counts. Index = tag value (0..=11). Indexed as u8 -> usize.
// Matches the TAG_* constants in value.rs. Size 16 gives headroom.
static ALIVE_BY_TAG: [AtomicU64; 16] = [
    AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
    AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
    AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
    AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
];

fn tag_name(tag: u8) -> &'static str {
    match tag {
        0 => "nil",
        1 => "int",
        2 => "float",
        3 => "bool",
        4 => "str",
        5 => "list",
        6 => "map",
        7 => "variant",
        8 => "closure",
        9 => "unit",
        10 => "bytes",
        11 => "partial_app",
        _ => "?",
    }
}

fn init_once() {
    if TRACE_INITIALIZED.swap(true, Relaxed) {
        return;
    }
    let enabled = std::env::var("AIRL_RT_TRACE").ok().as_deref() == Some("1");
    TRACE_ENABLED.store(enabled, Relaxed);
    if !enabled {
        return;
    }
    // Spawn a background thread that prints stats every 2 seconds.
    // Thread exits when the process exits (it's a background thread).
    std::thread::spawn(|| {
        eprintln!("[rt-trace] started (AIRL_RT_TRACE=1)");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let a = ALLOCS.load(Relaxed);
            let f = FREES.load(Relaxed);
            let rss = rss_mib();
            // Build "tag=count tag=count" for the top ~5 live tags.
            let mut per_tag: Vec<(u8, u64)> = (0..12u8)
                .map(|t| (t, ALIVE_BY_TAG[t as usize].load(Relaxed)))
                .filter(|(_, c)| *c > 0)
                .collect();
            per_tag.sort_by(|a, b| b.1.cmp(&a.1));
            let tags_s: String = per_tag.iter().take(6)
                .map(|(t, c)| format!("{}={}", tag_name(*t), c))
                .collect::<Vec<_>>()
                .join(" ");
            eprintln!(
                "[rt-trace] rss={}MiB alive={} allocs={} freed={}  {}",
                rss, a.saturating_sub(f), a, f, tags_s
            );
        }
    });
}

#[cfg(target_os = "linux")]
fn rss_mib() -> u64 {
    if let Ok(s) = std::fs::read_to_string("/proc/self/status") {
        for line in s.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                if let Some(v) = rest.split_whitespace().next() {
                    if let Ok(kib) = v.parse::<u64>() {
                        return kib / 1024;
                    }
                }
            }
        }
    }
    0
}

#[cfg(not(target_os = "linux"))]
fn rss_mib() -> u64 {
    0
}

#[inline]
pub fn on_alloc(tag: u8) {
    ALLOCS.fetch_add(1, Relaxed);
    if (tag as usize) < ALIVE_BY_TAG.len() {
        ALIVE_BY_TAG[tag as usize].fetch_add(1, Relaxed);
    }
    if !TRACE_INITIALIZED.load(Relaxed) {
        init_once();
    }
}

#[inline]
pub fn on_free(tag: u8) {
    FREES.fetch_add(1, Relaxed);
    if (tag as usize) < ALIVE_BY_TAG.len() {
        ALIVE_BY_TAG[tag as usize].fetch_sub(1, Relaxed);
    }
}
