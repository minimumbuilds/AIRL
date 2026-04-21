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
    // Register an atexit handler to dump per-site stats on normal exit.
    // OOM-killed processes won't reach it, but normal exits (and panics
    // after main returns) will. Guarded by `rt_trace_sites` + TRACE_ENABLED.
    #[cfg(feature = "rt_trace_sites")]
    {
        extern "C" fn print_sites_on_exit() {
            dump_sites();
        }
        unsafe { libc::atexit(print_sites_on_exit); }
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

// ── Allocation-site tagging (spec 4) ─────────────────────────────────
//
// Feature-gated under `rt_trace_sites`. Each call site that allocates an
// RtValue can register a stable string name and receive a small u16 id;
// that id is stored on the RtValue and used to bump / decrement a per-site
// alive counter. On process exit (or periodic dump) we print the top-N
// sites sorted by alive count — turns "I see 1.6M leaked lists" into
// "site #42 (list.rs:airl_append.clone-path) has 800K alive".

#[cfg(feature = "rt_trace_sites")]
mod sites {
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed};
    use std::sync::{OnceLock, RwLock};

    // Site registry. Index = site_id (u16). Entry = (name, alive_counter).
    // RwLock on the Vec because registration is rare (O(#unique call sites),
    // a few dozen total) but reads on every alloc/free would serialize under
    // a Mutex. Atomics for the alive counters avoid any lock on the hot path.
    static REGISTRY: OnceLock<RwLock<Vec<(&'static str, AtomicU64, AtomicU64)>>> = OnceLock::new();
    // Separate atomic for "next available id" so we can issue ids without
    // holding the write lock the whole time.
    static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
    // Id 0 is reserved for "unknown" — allocations that didn't get an
    // explicit site (e.g., legacy alloc() callers during migration).

    fn registry() -> &'static RwLock<Vec<(&'static str, AtomicU64, AtomicU64)>> {
        REGISTRY.get_or_init(|| {
            let mut v = Vec::with_capacity(64);
            v.push(("<unknown>", AtomicU64::new(0), AtomicU64::new(0)));
            RwLock::new(v)
        })
    }

    /// Register an allocation site and return a stable u16 id for it.
    /// Safe to call many times with the same name — id is memoized via
    /// the caller's OnceLock<u16>. Returns 0 if the registry is full
    /// (u16 space exhausted — shouldn't happen in practice).
    pub fn register(name: &'static str) -> u16 {
        let id = NEXT_ID.fetch_add(1, Relaxed);
        if id > u16::MAX as usize {
            return 0;
        }
        let reg = registry();
        let mut w = reg.write().unwrap();
        // Re-check NEXT_ID under write lock in case another thread raced.
        while w.len() <= id {
            w.push(("", AtomicU64::new(0), AtomicU64::new(0)));
        }
        w[id] = (name, AtomicU64::new(0), AtomicU64::new(0));
        id as u16
    }

    pub fn on_alloc(site_id: u16) {
        let reg = registry();
        let r = reg.read().unwrap();
        if let Some(entry) = r.get(site_id as usize) {
            entry.1.fetch_add(1, Relaxed);
        }
    }

    pub fn on_free(site_id: u16) {
        let reg = registry();
        let r = reg.read().unwrap();
        if let Some(entry) = r.get(site_id as usize) {
            entry.2.fetch_add(1, Relaxed);
        }
    }

    /// Dump the top-N sites by alive count. Called on process exit.
    pub fn dump() {
        let reg = registry();
        let r = reg.read().unwrap();
        let mut rows: Vec<(u16, &'static str, u64, u64, i64)> = r.iter().enumerate()
            .filter_map(|(i, (name, a, f))| {
                if i == 0 && name.is_empty() { return None; }
                let alloc = a.load(Relaxed);
                let freed = f.load(Relaxed);
                let alive = alloc as i64 - freed as i64;
                if alloc == 0 { return None; }
                Some((i as u16, *name, alloc, freed, alive))
            })
            .collect();
        rows.sort_by(|a, b| b.4.cmp(&a.4));
        eprintln!("[rt-trace-sites] top allocation sites (sorted by alive):");
        eprintln!("  {:>4}  {:<48}  {:>10}  {:>10}  {:>10}", "rank", "site", "alive", "allocs", "freed");
        for (rank, (_id, name, allocs, freed, alive)) in rows.iter().take(30).enumerate() {
            eprintln!("  {:>4}  {:<48}  {:>10}  {:>10}  {:>10}", rank + 1, name, alive, allocs, freed);
        }
    }
}

/// Register an allocation site name and receive a u16 id. Memoize the id in
/// a module-static `OnceLock<u16>` at the call site — don't call this on
/// every allocation. Returns 0 when the feature is compiled out.
#[cfg(feature = "rt_trace_sites")]
#[inline]
pub fn register_site(name: &'static str) -> u16 {
    sites::register(name)
}

#[cfg(not(feature = "rt_trace_sites"))]
#[inline]
pub fn register_site(_name: &'static str) -> u16 {
    0
}

/// Bump the alive counter for a site on allocation.
#[cfg(feature = "rt_trace_sites")]
#[inline]
pub fn on_alloc_at_site(site_id: u16) {
    sites::on_alloc(site_id);
}

#[cfg(not(feature = "rt_trace_sites"))]
#[inline]
pub fn on_alloc_at_site(_site_id: u16) {}

/// Decrement the alive counter for a site on free.
#[cfg(feature = "rt_trace_sites")]
#[inline]
pub fn on_free_at_site(site_id: u16) {
    sites::on_free(site_id);
}

#[cfg(not(feature = "rt_trace_sites"))]
#[inline]
pub fn on_free_at_site(_site_id: u16) {}

/// Dump the per-site summary. Called from `main`'s cleanup or via atexit.
#[cfg(feature = "rt_trace_sites")]
pub fn dump_sites() {
    if TRACE_ENABLED.load(Relaxed) {
        sites::dump();
    }
}

#[cfg(not(feature = "rt_trace_sites"))]
pub fn dump_sites() {}
