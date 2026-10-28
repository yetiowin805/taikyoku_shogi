# Second-generation search entrants — 27 October 2026

The user authorized launching the proposed eight search trials and two network transfers through the existing tournament admission system. Main implementation starts at `ee9356e2035aed4ad128e8612ad3aef058ecf93c`. Deployment uses an isolated backport onto the exact running engine `dcb54bd31db8e5b1693ac9e39e4fab76e0421106`; it must preserve the legacy behavior of all existing identities. No network training, scheduler changes, manual retirements, or preliminary strength tournament are part of this launch.

## Explicit policies

`HLF` combines history/tactical LMR with the opt-in QFIX package. F already applies globally on main; it remains opt-in on the deployed legacy branch. `HLFN` adds the existing non-PV/static-eval null eligibility guard. `FN` applies F+N without H or L. `HLFS` adds the existing root/PV sacrifice probes, without broadening their eligibility. The two other networks (`384v3` and `384v4.5`) receive HLF using their own exact original NNUE and material bytes.

Four new mechanisms stay separate:

- **HLF_CH:** one-previous-move continuation history, ordering only. A 65,536-slot tagged direct table occupies 1 MiB of entry payload on the deployment ABI; the table is allocated only for CH. Fingerprints include mover/color, endpoints, promotion and all two-step/FreeEagle route positions. Bounded updates use H's gravity rule and per-iteration halving; collisions affect hints only. Histories are per search, retained through iterative deepening, not across games. Previous-move identity is restored across recursion and cleared on null passes.
- **HLF_HR:** restore one quiet LMR ply when global quiet history is at least 512 and the move has at least two positive cutoff updates. L's existing protection can restore another reduced ply; reduction cannot fall below zero and never increases. Root and interior rules match. Support keys/cap follow existing H quiet history; this deliberately does not change its endpoint identity.
- **HLF_QR:** after an originally one-ply scout q capture leaves a major on the previous landing with a generated destination recapture, permit one additional deep q ply. Configured q=0/1 and ordinary full-depth PV q are unchanged. Existing deep candidate filters and royal/promotion exceptions remain. The exact entry budget and spent bit distinguish TT results. The spent state survives forced evasions and is restored for sibling branches, bounding the extra allowance to once per path within that q entry.
- **HLFN_V:** after a shallow static null fail-high, verify only following a loud exchange or credible destination recapture of a major. The real-position verification is depth minus one, narrow-window, with passing disabled throughout the verification subtree. Main and q TT keys use a separate verification namespace, so null-enabled scores cannot supply the witness. Retry context is restored; an unsuccessful verification resumes ordinary search. This is selective verification, not proof of a game-theoretic bound.

All experimental policies remain explicitly named in checkpoint metadata and engine capability output. Baseline defaults are unchanged. Remove losing policies after adjudication, as required by AGENTS.md.

## Admission and validation

[admission.json](admission.json) records all ten immutable identities and their bases. The shared admission preparer now accepts either a string policy using the default base or an explicit `{base_entrant, experiment}` object. Duplicate (base, policy) pairs fail; the same package can transfer to different networks in one atomic coordinator request. Existing single-base configs remain accepted. Every source NNUE hash is checked before writing candidate descriptors; frozen descriptors and networks are pinned by the existing transaction.

Validation and publication receipts are added after the exact builds complete. Unit tests are correctness/eligibility checks, not evidence of playing strength. Cost labels in the preceding proposal remain hypotheses until measured; the tournament provides the actual strength experiment.
