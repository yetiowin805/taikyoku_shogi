# Completed matched-compute tactical comparison

Completed October 19, 2026: 36 strategy runs and 12 complete follow-up audits,
without failures or resumed timing arms. All episodes bound NNUE_W384_v4.5.
Worker revision `5eb60570c8925988014fc87254e5cc1b004b6a3f`, SHA256
`9a5c1560bafcfd9597014572f67112a5ac0cb405aa73cd71c27c31504f9b2270`;
runner revision `159bf0b082ac3faf1d796f975254707973023ea2`.
The fixed corpus and methods are in `corpus.json` and `training/tactical_tree/README.md`.
Raw private results remain at `/opt/tactical-comparison-20261018/results` on the VPS;
the retrieved comparison is `data/analysis/tactical-comparison-20261018/comparison.json`.

At the 420 CPU-second checkpoint, adaptive root deepening changed 5/36 recommendations
relative to uniform deepening: 1/18 in fresh cases and 4/18 in diagnostic cases.
Branch exploration changed 24/36: 11/18 fresh and 13/18 diagnostic.
Neither method produced a >=1,000-unit improvement at both matched odd and even
child-search depths, at any of the 60/180/420-second checkpoints.
At 420 seconds adaptive had one >=1,000-unit regression on both parities;
branches had three. These are checked positions, not independent games or Elo estimates.

After eight rollout replies, equal-case mean gains versus uniform were +1.5
for adaptive and -24.1 for branches in fresh cases, and -249.1 / -973.3 in
diagnostic cases. The underlying observations are correlated, sometimes shallow,
and omit incomparable terminal/numeric endpoints; these means are descriptive only.
Terminal outcomes were never converted into arbitrary numeric gains.

Three useful diagnostics at the longest checkpoint:

- Slot 673 ply 174: the branch proposal was worse by 2,380 / 1,573 at matched
  child depths 3/4, and worse by 3,434 after eight rollout replies.
- Slot 756 ply 334: both alternative methods selected a proposal worse by
  4,128 / 2,457 at child depths 3/4, and worse by 3,440 after eight replies.
- Slot 817 ply 43: the branch proposal was worse by 3,927 / 1,751 at child
  depths 3/4, and worse by 6,793 after eight replies.

The partial-tree backup can promote unsearched heuristic root candidates, and
mixing different depth horizons can make parity offsets resemble discoveries.
Slot 1337 ply 1024 remained +712, -6,776, +695, -6,797, +647 at depths 1–5.
At slot 1448 ply 1394, a branch recommendation led to a terminal Black win in
the short continuation; the baseline's deeper child searches also indicated
a Black win, so this does not establish that a saving defense existed.

The experiment does not establish true blunder locations, superior playing
strength, or the impossibility of useful branch exploration. On this corpus,
uniform deepening remains the better-supported baseline. Before another branch
policy trial, independently verify promoted candidates and investigate horizon
consistency. No hypothetical labels or production changes were introduced.
