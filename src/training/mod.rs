//! Local Texel-style training pipeline (pre-cloud).
//!
//! CLI and data layout: `src/training/README.md`. Cloud workers: `deploy/README.md`.

pub mod cli;
pub mod eval_trace;
pub mod featurize;
pub mod file_pst_grid;
pub mod hang_q_ab_grid;
pub mod history;
pub mod knockout;
pub mod loud_grid;
pub mod match_harness;
pub mod mobility_seed;
pub mod paths;
pub mod pool;
pub mod pst_grid;
pub mod record;
pub mod run_status;
pub mod scale_sample;
pub mod start_gen;
pub mod texel;
pub mod top4_mix_grid;
pub mod tournament;
pub mod two_mob_grid;
pub mod two_mob_q_grid;
pub mod worker;
