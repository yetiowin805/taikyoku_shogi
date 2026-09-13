//! Compile-time defaults with thread-local controls only in experimental builds.
//! Configure before constructing a position; normal binaries expose no switches.
pub const TACTICAL: u32 = 1 << 0;
pub const BOARD_INDEX: u32 = 1 << 1;
pub const STORAGE: u32 = 1 << 2;
pub const REPETITION: u32 = 1 << 3;
pub const BORROW_ATTACKERS: u32 = 1 << 4;
pub const ROOT_INDEX: u32 = 1 << 5;
pub const MOBILITY_COUNT: u32 = 1 << 6;
pub const LAZY_LABELS: u32 = 1 << 7;
pub const DIRECTED: u32 = 1 << 8;
pub const ALL: u32 = (1 << 9) - 1;

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub mechanical: u32,
    pub swap_remove: bool,
    pub incremental_mobility: bool,
    pub tt_first: u8,
    pub interior_pvs: bool,
    pub aspiration: i32,
    pub retain_tt: u8,
    pub clock_interval: u64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            // Retained workspace capacity exceeded the pilot memory budget.
            // Keep the prototype measurable, but do not enable it by default.
            mechanical: ALL & !STORAGE,
            swap_remove: false,
            incremental_mobility: false,
            tt_first: 0,
            interior_pvs: false,
            aspiration: 0,
            retain_tt: 0,
            clock_interval: 1,
        }
    }
}
#[cfg(feature = "search-experiments")]
thread_local! { static OPTIONS: std::cell::Cell<Options> = std::cell::Cell::new(Options::default()); }
#[cfg(feature = "search-experiments")]
pub fn configure(options: Options) {
    OPTIONS.with(|o| o.set(options));
}
#[inline]
pub fn options() -> Options {
    #[cfg(feature = "search-experiments")]
    {
        OPTIONS.with(|o| o.get())
    }
    #[cfg(not(feature = "search-experiments"))]
    {
        Options::default()
    }
}
#[inline]
pub fn enabled(flag: u32) -> bool {
    options().mechanical & flag != 0
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct Counters {
    pub interior_probes: u64,
    pub interior_researches: u64,
    pub aspiration_retries: u64,
    pub tt_first_probes: u64,
    pub tt_first_cutoffs: u64,
    pub retained_hints: u64,
    pub retained_bounds: u64,
    pub rejected_bounds: u64,
    pub root_probes: u64,
    pub root_researches: u64,
}
#[cfg(feature = "search-experiments")]
thread_local! { static COUNTERS: std::cell::Cell<Counters> = const { std::cell::Cell::new(Counters { interior_probes: 0, interior_researches: 0, aspiration_retries: 0, tt_first_probes: 0, tt_first_cutoffs: 0, retained_hints: 0, retained_bounds: 0, rejected_bounds: 0, root_probes: 0, root_researches: 0 }) }; }
#[inline]
pub fn count(f: impl FnOnce(&mut Counters)) {
    #[cfg(feature = "search-experiments")]
    {
        COUNTERS.with(|c| {
            let mut v = c.get();
            f(&mut v);
            c.set(v);
        });
    }
    #[cfg(not(feature = "search-experiments"))]
    {
        let _ = f;
    }
}
pub fn counters() -> Counters {
    #[cfg(feature = "search-experiments")]
    {
        COUNTERS.with(|c| c.get())
    }
    #[cfg(not(feature = "search-experiments"))]
    {
        Counters::default()
    }
}
