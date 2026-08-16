//! Optional wall-time buckets for search-speed profiling (`--features search-profile`).

use std::cell::Cell;
use std::time::Instant;

thread_local! {
    static EVAL_NS: Cell<u128> = const { Cell::new(0) };
    static GEN_NS: Cell<u128> = const { Cell::new(0) };
    static TWO_STEP_NS: Cell<u128> = const { Cell::new(0) };
    static STANDARD_GEN_NS: Cell<u128> = const { Cell::new(0) };
    static FE_GEN_NS: Cell<u128> = const { Cell::new(0) };
    static ATK_NS: Cell<u128> = const { Cell::new(0) };
    static MAKE_NS: Cell<u128> = const { Cell::new(0) };
    static ORDER_NS: Cell<u128> = const { Cell::new(0) };
    static FILTER_NS: Cell<u128> = const { Cell::new(0) };
}

#[derive(Debug, Clone, Copy)]
pub struct Report {
    pub eval_ns: u128,
    pub gen_ns: u128,
    pub two_step_ns: u128,
    pub standard_gen_ns: u128,
    pub fe_gen_ns: u128,
    pub atk_ns: u128,
    pub make_ns: u128,
    pub order_ns: u128,
    pub filter_ns: u128,
}

pub fn reset() {
    EVAL_NS.with(|c| c.set(0));
    GEN_NS.with(|c| c.set(0));
    TWO_STEP_NS.with(|c| c.set(0));
    STANDARD_GEN_NS.with(|c| c.set(0));
    FE_GEN_NS.with(|c| c.set(0));
    ATK_NS.with(|c| c.set(0));
    MAKE_NS.with(|c| c.set(0));
    ORDER_NS.with(|c| c.set(0));
    FILTER_NS.with(|c| c.set(0));
}

pub fn report() -> Report {
    Report {
        eval_ns: EVAL_NS.with(|c| c.get()),
        gen_ns: GEN_NS.with(|c| c.get()),
        two_step_ns: TWO_STEP_NS.with(|c| c.get()),
        standard_gen_ns: STANDARD_GEN_NS.with(|c| c.get()),
        fe_gen_ns: FE_GEN_NS.with(|c| c.get()),
        atk_ns: ATK_NS.with(|c| c.get()),
        make_ns: MAKE_NS.with(|c| c.get()),
        order_ns: ORDER_NS.with(|c| c.get()),
        filter_ns: FILTER_NS.with(|c| c.get()),
    }
}

pub struct Scope {
    start: Instant,
    dest: fn(u128),
}

impl Drop for Scope {
    fn drop(&mut self) {
        (self.dest)(self.start.elapsed().as_nanos());
    }
}

fn add_eval(ns: u128) {
    EVAL_NS.with(|c| c.set(c.get().saturating_add(ns)));
}
fn add_gen(ns: u128) {
    GEN_NS.with(|c| c.set(c.get().saturating_add(ns)));
}
fn add_two_step(ns: u128) {
    TWO_STEP_NS.with(|c| c.set(c.get().saturating_add(ns)));
}
fn add_standard_gen(ns: u128) {
    STANDARD_GEN_NS.with(|c| c.set(c.get().saturating_add(ns)));
}
fn add_fe_gen(ns: u128) {
    FE_GEN_NS.with(|c| c.set(c.get().saturating_add(ns)));
}
fn add_atk(ns: u128) {
    ATK_NS.with(|c| c.set(c.get().saturating_add(ns)));
}
fn add_make(ns: u128) {
    MAKE_NS.with(|c| c.set(c.get().saturating_add(ns)));
}
fn add_order(ns: u128) {
    ORDER_NS.with(|c| c.set(c.get().saturating_add(ns)));
}
fn add_filter(ns: u128) {
    FILTER_NS.with(|c| c.set(c.get().saturating_add(ns)));
}

pub fn eval_scope() -> Scope {
    Scope {
        start: Instant::now(),
        dest: add_eval,
    }
}
pub fn gen_scope() -> Scope {
    Scope {
        start: Instant::now(),
        dest: add_gen,
    }
}
pub fn two_step_scope() -> Scope {
    Scope {
        start: Instant::now(),
        dest: add_two_step,
    }
}
pub fn standard_gen_scope() -> Scope {
    Scope {
        start: Instant::now(),
        dest: add_standard_gen,
    }
}
pub fn fe_gen_scope() -> Scope {
    Scope {
        start: Instant::now(),
        dest: add_fe_gen,
    }
}
pub fn atk_scope() -> Scope {
    Scope {
        start: Instant::now(),
        dest: add_atk,
    }
}
pub fn make_scope() -> Scope {
    Scope {
        start: Instant::now(),
        dest: add_make,
    }
}
pub fn order_scope() -> Scope {
    Scope {
        start: Instant::now(),
        dest: add_order,
    }
}
pub fn filter_scope() -> Scope {
    Scope {
        start: Instant::now(),
        dest: add_filter,
    }
}
