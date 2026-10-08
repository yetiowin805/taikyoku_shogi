# Initial screening build

The initial feature build was produced before the later regression tests and
`TACTICAL_NO_SOFT_STOP` switch. Its files are retained to make the recorded
source hashes reproducible. Export commit
`66d47bec09a52ac1c969ebfb0664ae19cf6dfce2` into a separate directory, apply
`gzip -dc tracked.patch.gz | git apply`, then copy `tactical.rs` to `src/search/tactical.rs` and
`tactical_probe.rs` to `src/bin/tactical_probe.rs`. Compare their SHA-256 hashes
with the relevant run manifest before rebuilding.

The later measured source is commit `f5c390d`. The final branch additionally
pins the diagnostic CLI's q depth against ambient environment overrides; all
recorded configuration events already have the intended q depth.
