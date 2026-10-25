# Search and quiescence after NNUE: a comparative research report

**25 October 2026.** Source audit and research agenda; no new speed or strength experiments. Local baseline: `3a2044085acb4c4c7828b1812b8052fe917b7b04` (current `main` when this survey began). Reference agent: **NNUE W512 v4.5**, as requested. External engine snapshots are pinned in [sources.json](sources.json). The earlier [W512 experiment report][previous] remains evidence about its recorded revision and workload, not a measurement of today's recommendations.

## Main conclusions

There is substantial room to investigate our search. The strongest opportunity is to improve **which moves get enough search**, especially at shallow depth, rather than simply increase the reported depth or add every technique found in Stockfish.

Our engine has iterative deepening, aspiration windows, root principal variation search (PVS), late move reductions (LMR), null-move pruning, transposition tables (TTs), killers/history, staged generation, and selective quiescence. It is not a bare alpha-beta implementation. However, several decisions are coarse: quiet moves receive fixed reductions by list position; apparently hanging expensive captures can be discarded; quiescence admission depends on specific material/threat gates; and most non-PV leaves receive only one q ply. Those are promising places to investigate tactical blindness. This is an inference from [current main search][local-ab], [root search][local-root], and [leaf/q policy][local-leaf], not evidence that changing any one will improve strength.

The highest-priority research directions are:

1. **Trace why a critical move was missed.** Was it never generated, excluded by q admission, cut by a material rule, reduced to a static leaf, rejected by null move, or never reached before time expired? Different causes need different fixes.
2. **Replace selected hard exclusions with cheap verification.** Start with high-value hanging captures and quiet tactical defenses near the frontier. A speculative sacrifice need not be searched at full depth everywhere, but unconditional omission is a strong assumption.
3. **Make q admission and continuation sensitive to forcing threats.** Detect royal danger, newly opened attack routes, credible countercaptures and promotions. Spend extra q work on these events, while keeping ordinary quiet positions cheap.
4. **Improve quiet ordering and LMR together.** Better histories, route-aware identities, and protection for credible defenses can give the right quiet move enough depth without widening the entire tree.
5. **Revisit null-move eligibility and shallow fail-high verification.** Our current pass search can end in bare evaluation at depths that matter in normal games.
6. **Save generation and state-update work before micro-optimizing q sorting.** A TT-first picker and direct tactical generation have plausible scope, but earlier versions were not established wins and existing NNUE accelerations must be credited.
7. **Measure decision quality and clock use.** A change that saves 2% need not add a whole ply on sixteen positions; a change that adds a ply need not search the important line better.

These are proposed priorities, with risks and evaluation plans below. No new flags, search defaults, network weights, or production behavior are changed by this report.

## How to read the evidence

- **Observed implementation:** verified in the linked source at a fixed commit. This establishes what a mechanism does, not its individual Elo value.
- **Measured locally:** an earlier experiment, always accompanied by its comparator, workload and limitations.
- **Proposed:** an adaptation or hypothesis for Taikyoku. Expected cost and benefit are qualitative unless explicitly calculated.
- **Audit item:** a source-level concern that deserves a targeted test; it is not a demonstrated game-losing bug.

Stockfish, YaneuraOu and Pikafish share substantial search ancestry. Their agreement is useful precedent, not three independent experiments proving a technique. Berserk and public Ethereal offer additional implementations. Stockfish 11 supplies a concrete handcrafted-era comparison. I inspected source rather than relying on tutorial descriptions; some comments describe older algorithms that the current executable path no longer implements.

## What our current engine actually does

### Main search

The root searches increasing depths, initially using a ±500 internal-unit aspiration window after the first iteration, widening on failure. The first root candidate gets a full window; later candidates get narrow scouts and a full-depth/full-window retry when they beat the threshold. Root quiet moves can be reduced from the fourth list entry, with a larger reduction from the thirteenth. Captures and loud promotions are exempt from this quiet reduction. [Root implementation][local-root].

Inside the tree, main TT results can cut off at both PV and non-PV nodes. Null move is available from remaining depth two, avoids consecutive nulls and last-royal check, and uses a bare static evaluation when its reduced child depth is zero. There is no explicit PV restriction or static-eval-versus-beta eligibility gate in this implementation. Move generation first produces captures and ordinary quiet moves, then quiet multi-leg moves if needed. The TT move is preferred within the generated stage, rather than tried before generating the stage. [Main search][local-ab].

Interior quiet LMR begins at remaining depth two and list index three, excludes captures, loud promotions and killers, and increases from one to two plies at index twelve, capped by remaining depth. A promising reduced result is retried. This makes list order consequential: a nominal depth-five search can give a late quiet line much less than five plies before leaf/q rules take over. The index counts list entries, including entries that may subsequently be skipped; it is not simply the number of fully searched moves. [Move loop][local-loop].

A separate high-value capture test can **skip a move entirely** if the landing is attacked and immediate net gain is too small relative to the mover. Last-royal evasions and royal captures have protections. Path/multi-leg captures get post-move attack confirmation where required. The test still is not a complete alternating exchange search or a proof that a sacrifice fails. [Hang filter][local-hang].

Quiet history is a per-search `(from,to)` map with positive depth-squared increments and a ceiling; there are two killers per ply. There is no continuation-history family or learned capture history in this path. Main TT, q TT, history and killers are logically fresh each search, although allocation and NNUE caches can be reused. [History and identities][local-keys], [context construction][local-context].

### Quiescence

Forced last-royal defense is checked before the ordinary leaf exit, and again at recursive q nodes before TT lookup, stand pat or the zero-q-budget return. All generated evasions, including quiet flights and hanging saving captures, remain candidates. An optional scarce-defense extension exists, but **the inspected W512 v4.5 descriptor uses `last_royal_mode: legacy`**, so that extension and the optional root mate probe are not active for this reference agent. [Evasion handling][local-leaf], [descriptor provenance](sources.json).

For ordinary leaves, PV nodes use the configured q budget; non-PV nodes cap it at one. W512 v4.5 configures q depth two and `q_own_large_only=true`. Its depth field in the descriptor is two, but runtime depth/time options can override that; this report uses the user's approximate in-game depth-five workload as the target, not as a freshly measured distribution. [Leaf depth][local-loop], [descriptor provenance](sources.json).

Capture q opens after sufficiently loud captures, a possible destination take of a large previous mover, hanging-large-piece opportunities, or royal-capture opportunities. Loud promotions can open a promotion-only search. A quiet-looking leaf can therefore return static NNUE without examining every legal capture. Once open, S2 keeps large destination victims plus royal captures and loud promotions. PathAware has further recapture/path restrictions, material delta pruning, and hang filters. Its ordinary first-ply cap is two, deeper cap three, with exceptions for royal takes, loud promotions and certain major recaptures. Deeper generation targets the previous landing instead of repeatedly enumerating the whole army. [Entry gates][local-leaf], [candidate generation and pruning][local-q].

The q TT key already incorporates previous landing, exact q budget, entry budget, path permission, capture inclusion, wipe context and royal-extension state. This is important progress from older q correctness work. The main TT does not encode the PV-dependent leaf horizon in its key; cross-policy reuse remains an audit topic, not a claim that every reuse is invalid. [Q keys][local-keys], [main TT cutoffs][local-ab].

### Existing investigation infrastructure

The repository already has a tactical-tree worker and Python analysis pipeline that preserve full routes and game history, record completed iterations, explore alternative continuations and separate heuristic values from terminal-outcome proofs. Extend this infrastructure for exclusion counters and broader-search comparisons instead of building a duplicate runner. Its current worker invokes the production search policy, so it is not by itself a less-pruned tactical oracle. No results from that separate ongoing project are imported into this survey. [Tactical-tree protocol][local-tactical-doc], [worker][local-tactical-worker].

### Existing execution improvements

Production already has substantial NNUE engineering: exact SIMD dense kernels, reusable accumulator/undo storage, bounded feature-row and move-delta caches, memoized attack/leaf queries, feature-row sharing, root accumulator reuse and capture-query caching. Native builds and optional compiler PGO have also been investigated. Recommending these as wholly new opportunities would duplicate previous work. Remaining proposals should be compared against this baseline. [NNUE follow-up][speed-first], [second-round results][speed-second].

## What established engines demonstrate

### Modern Stockfish: selective search with several layers of evidence

The pinned October 2026 source uses PV scouts/re-search, history-dependent reductions, pruning margins informed by position/search context, and null-move verification at sufficiently high depth. It also uses ProbCut and singular extensions. These techniques form an interacting policy, so their constants or historical gain annotations cannot be imported as Taikyoku predictions. [Search implementation][sf-search].

Its qsearch is entered at the main-search frontier and recursively explores tactical moves. It distinguishes check evasions from ordinary stand pat, uses SEE and futility, and conditionally prunes after two moves with exceptions including checks, recaptures and promotions. It has no ordinary fixed two-ply q budget like ours. The current picker does not have the old dedicated quiet-check q stage. [Q implementation][sf-q], [picker][sf-picker].

The move picker uses capture history and several quiet histories, and separates promising captures, quiets and deferred bad moves. It can return a validated TT move first. The useful lesson is to prioritize evidence about a move and postpone avoidable work, not that all quiets are unimportant. [Move scoring and staging][sf-picker].

### Stockfish 11: useful handcrafted-era counterexample

This release predates NNUE and already has PVS, LMR, null move, futility, ProbCut and singular extensions. These techniques are not uniquely neural-network algorithms. Its q picker explicitly has a quiet-check stage at a designated q depth and a deeper recapture restriction. Modern Stockfish changed that design; there is no timeless rule that all good engines search every check in q, or that all have abandoned it. [SF11 search][sf11-search], [SF11 picker][sf11-picker].

### YaneuraOu: shogi-specific forcing moves and repetition

The inspected standard engine combines selective alpha-beta with staged move picking, check evasions, repetition adjudication and specialized mate-in-one probes under guarded conditions. Shogi's drops and repeated checks make tactical expansion particularly sensitive to branching and rules. The active q picker calls `CAPTURES`/`CAPTURES_ALL`; the separately defined `CAPTURES_PRO_PLUS` category can include quiet pawn promotions, but that is not the generator called here. Do not infer the active move set from an old scoring comment. [Search][yo-search], [picker][yo-picker], [generator definitions][yo-movegen].

A source-reading trap matters here: the large introductory q comment still describes old q-depth check and recapture stages. The current q signature has no such depth argument, and its active picker uses q-capture/evasion stages. The report does **not** present those old comments as current behavior. [Q implementation][yo-q], [active picker][yo-picker].

The transferable idea is narrowly targeted tactical detection with correct rule handling, not unconditional expansion of every forcing-looking move. A shogi mate-in-one optimization is precedent for specialized detectors, not proof our more complex royal rules admit the same cheap detector.

### Pikafish: game rules belong inside search

Pikafish applies the Stockfish family of techniques to Xiangqi, but repetition adjudication and terminal handling are game-specific. Its qsearch consults `rule_judge`, and no legal move can be a loss even without check. This is a useful warning against importing chess stalemate, pass-move or repetition assumptions into our game. Its success as an NNUE engine does not remove the need for explicit rule-aware search. [Pikafish qsearch][pika-q], [rule implementation][pika-position].

### Berserk: bounded quiet checks remain a viable design choice

Berserk's qsearch permits a quiet-check generation stage near q entry (`depth >= -1` in this revision), with separate evasion handling and SEE-based pruning. Its main search uses history-dependent LMR and other selective techniques, while its q evaluation includes correction-history adjustments. This is a concrete alternative to modern Stockfish's q move set, not evidence that quiet checks will improve our engine. [Berserk search][berserk-search], [q picker][berserk-picker], [histories][berserk-history].

### Public Ethereal: inspect the caller as well as qsearch

Public Ethereal provides history-guided reductions, capture history, SEE and a staged picker. The normal search enters q at non-check leaves; check handling cannot be understood from the q routine alone. Its evaluation code has both NNUE and handcrafted paths. This is the public source snapshot, not a claim about undisclosed commercial versions. Comments containing historical Elo estimates are not causal estimates for our workload. [Search][eth-search], [picker][eth-picker], [evaluation][eth-eval].

## What NNUE changes, and what it does not

NNUE supplies a fast learned position evaluation. It does not guarantee that an interrupted exchange, a sacrifice, a royal threat or a multi-leg route has been tactically resolved. Alpha-beta still needs trustworthy leaf behavior and useful move ordering. Efficient incremental updates change the cost balance, but do not turn static scores into proofs. The official NNUE description explains the sparse-input/incremental-update design and the choice between eager and deferred updates. [NNUE documentation][nnue-doc].

For our engine, four consequences follow:

- **Recalibrate material-based pruning.** `stand_pat + material_gain` ignores changes in the learned residual and threats. Treat it as a heuristic unless a genuine bound on the complete evaluation is established. Estimate tail errors separately for quiet moves, promotions, simple captures and wipes; a mean error is insufficient for pruning safety.
- **Measure cost at the right places.** NNUE cost includes make/unmake updates and cache traffic, not only the final evaluate call. A search change can increase evaluations while decreasing total time, or the reverse.
- **Keep the network fixed while testing search.** Use 512v4.5 first. A larger or newer model changes both quality and speed, so simultaneous search/network changes obscure cause. Later test transfer to a second strong network.
- **Account for the training distribution.** A model trained largely on search-filtered positions may behave differently on noisy leaves exposed by a new q policy. Test the search change with fixed weights first, then consider retraining. No claim about v4.5's actual distribution is made without inspecting its training records.

Modern Stockfish also transforms raw NNUE output using search/game context, and uses correction histories separately. That shows evaluation and search are coupled; it does not justify adding an arbitrary bias to our scores. [Evaluation scaling][sf-eval], [history structures][sf-history].

## Quiescence options

The following are **proposals**, not measured improvements. Costs are relative expectations to validate on W512 v4.5. “Protected” means exempt from a particular heuristic, not automatically winning or legal.

### Q1 — Audit selective entry against an always-enter reference

Use a broader qsearch at every ordinary leaf as a diagnostic reference. Count disagreements with the current gates and classify the omitted tactic. This separates “q never opened” from “q opened but pruned the move.” Do not immediately deploy all-capture q: on this board, repeated unrelated exchanges can explode. A cheap entry query could still preserve most of the reference's useful decisions. Cost: potentially large diagnostic work; little permanent memory. Precedent: frontier entry in [Stockfish][sf-search] and [Ethereal][eth-search].

### Q2 — Threat-based entry after quiet moves

Add a narrow trigger for a newly exposed large attack, royal danger, a promotion threat or a multi-leg fork. Detect changes around the move's affected squares/rays rather than rescan every piece. A quiet move that creates a threat and a quiet move that fails to defend one both matter. Start by recording triggers without acting, then test the ones that predict missed refutations. Cost: attack bookkeeping; risk: the detector can cost more than the q work it saves. This is a Taikyoku adaptation.

### Q3 — Broaden S2 by consequence, not only victim size

Our large-destination-victim restriction can exclude capturing a small blocker or defender whose removal enables a decisive route. Retain S2 for ordinary material grabs, but admit moves that open a verified royal/large-piece attack or remove a critical defender. Preserve the entire route, including intermediate victims and friendly losses. Cost: classification and some extra q branches. Prefer an explicit exception over removing all filtering at once. Basis: [current S2 filter][local-q]; benefit remains hypothetical.

### Q4 — Replace fixed top-N with staged tactical eligibility

Try all protected tactics, then positive/near-neutral exchanges, then a bounded number of speculative candidates selected by history or threat value. Alternatively make the cap depend on score proximity and tactical instability. A hard cap can miss the third-ranked winning move; an unbounded list can waste the clock. Compare a cap-only ablation and an ordering-only ablation so their effects can be separated. Current Stockfish's move-count limit is itself conditional; copying the numeral two alone misses the point. [SF q][sf-q].

### Q5 — Continue a credible exchange past the fixed q horizon

Permit extra q plies while a contested major exchange or forced royal defense remains unresolved, with a hard overall ply/time safeguard. Conversely stop early when no meaningful continuation remains. Our engine already extends royal evasions beyond ordinary q-budget exhaustion; this proposal targets non-royal exchanges. Cost: heavy-tailed lines, usually modest memory per ply. Protect repetitions and count actual extra work. A recursive tactical search in established engines motivates the comparison, but our branching requires a bounded adaptation.

### Q6 — Test a common tactical horizon for PV and scouts

Run a small controlled comparison with the same q budget for PV and non-PV leaves, then a verification policy that gives selected scout results the richer horizon. A narrow-window result from q1 does not automatically answer the same question as q2. Full-window retry repairs promising scouts, but a false fail-low can hide a move permanently. This is directly relevant to the prior PVS result. Cost: potentially large if applied everywhere; focus on near-bound/unstable leaves. [Current leaf cap][local-loop], [previous experiment][previous].

### Q7 — Bounded quiet checks or royal threats

Try one generation of credible quiet checking moves near q entry, or a small class of immediate last-royal threats. Filter cheaply, validate legality, and cap quiet forcing extensions separately from captures. Our multiple-royal rules mean “attacks a royal” is not always equivalent to chess check. Do not extend every check indefinitely. Stockfish 11 and Berserk provide actual bounded-check precedents; current Stockfish does not require that design. [SF11 picker][sf11-picker], [Berserk picker][berserk-picker].

### Q8 — Add quiet defenses only when a concrete threat exists

Capture q cannot find a retreat, interposition, defender move or quiet counter-threat unless it is included. For a detected impending major loss, generate defenses to that threat and compare them with captures. Keep normal q capture-focused elsewhere. The challenge is defining a real threat without a full opponent search: an attacked piece may be tactically safe. Begin with locally verifiable threats and treat uncertain ones as an ordering signal. Cost: extra defense generation; likely more important for decision quality than q-node count.

### Q9 — Replace landing-attack tests with bounded exchange analysis

Static exchange evaluation (SEE) alternates captures to estimate whether an exchange loses material. Our multi-victim routes, friendly losses, promotions and changes in attack geometry break a simple chess SEE port. Start with threshold SEE for simple captures; use conservative “unknown” for complex routes, or a one/two-reply local verification. Never interpret unknown as losing. Use the result for ordering before pruning. SEE is a precedent in [Ethereal][eth-search] and [Stockfish's picker][sf-picker]; this proposed approximation is specific to our rules.

### Q10 — Recalibrate delta/futility margins for NNUE

Test a nonzero uncertainty margin and exceptions for threat-changing moves, rather than comparing only static score plus immediate material. Distinguish the node-wide best-gain cutoff from per-move delta pruning: changing one can be masked by the other. Loud promotions bypass several per-move cuts but still contribute a finite gain to the current node-wide futility test; comments saying “always searched” should not be interpreted more broadly than the executable path. [Current node/per-move pruning][local-q]. Cost: extra branches; aim to reduce false exclusions at the frontier.

### Q11 — Order q moves by expected refutation value

Keep immediate royal wins first, then use learned capture history, exchange safety and relevant recaptures rather than gross victim sum alone. A huge corridor wipe can be distracting if it loses the mover or permits a royal attack. With top-N pruning, ordering changes eligibility, not just speed. Record the rank of adjudicated critical moves before and after the change. Cost: small bounded history tables and lookup traffic. [Capture-history precedent][sf-picker], [current ordering][local-q].

### Q12 — Generate only the relevant tactical moves

Generate destination captures, royal captures, promotions and threat responses directly; share attack work between the entry gate and candidate generation. Deep q already targets a victim square, so the new opportunity is reducing duplicated first-ply discovery and annotation. Require complete full-route equivalence if advertised as a pure speed change. Selective generators that omit routes are a different experiment. Cost: implementation complexity and possibly small reusable indexes; potentially more scope than sorting. [Current generators][local-qgen], [earlier profile][previous].

### Q13 — Revisit stand pat in special cases

Stand pat assumes declining a tactical continuation is an acceptable lower bound for the selective search. It is disallowed in forced royal defense, but zugzwang or a compulsory unfavorable response can still defeat the general intuition. Test low-mobility/low-material exceptions and the existing deferred stand-pat treatment of major recaptures. Do not remove stand pat everywhere. Also do not let a favorable static score suppress a directly available terminal royal capture. [Current treatment][local-q]. Cost: extra search in rare positions; use targeted fixtures and real examples.

### Q14 — Separate q result confidence from an exact game value

A selective q result can be exact within its implemented move set without being a minimax solution of the game. Audit TT flags, eligibility context and PV reuse; consider a policy/version identity or restricted score reuse where horizons differ. Keep move-order hints even when a score cannot be reused. Current Stockfish stores q lower/upper bounds rather than a q exact bound in the inspected version. That is precedent to study, not a rule to copy blindly. [SF q storage][sf-q], [our q keys][local-keys].

## Finding tactics that matter around depth five

The most useful tactical catalog is organized around **how a decision changes the outcome**, rather than only piece value or spectacular captures. The following are proposed test categories; no prevalence is asserted.

- **Last-royal survival:** a quiet flight, block or otherwise losing capture is the only defense. Include attacks via intermediate squares and path removal, not just destination attacks.
- **Spare-royal transitions:** losing one royal is survivable but changes subsequent safety constraints; promotion can create a new royal. Test one-to-zero and two-to-one transitions separately.
- **Multi-victim exchanges:** immediate enemy gain minus friendly path losses, followed by recapture of the mover. Include the same endpoints with different routes.
- **Removal of a blocker or defender:** a small capture creates a much larger threat. This specifically challenges victim-size pruning.
- **Intermediate moves:** instead of recapturing on the last landing, play a check, promotion, other capture or threat first. This challenges recapture-only continuation.
- **Sacrifice with compensation:** a hanging mover creates a forced royal attack, deflection or promotion. This challenges main-search hang skipping and q SEE approximations.
- **Quiet defense against a major threat:** retreat, interpose, reinforce, close a route, or give a counter-threat. This challenges LMR and capture-only leaves.
- **Quiet preparation:** a move with no immediate material payoff makes the next threat unavoidable. Put the decisive reply at plies four, five and six to measure the frontier effect.
- **Promotion and underpromotion choices:** include gained movement capability, royal creation, and choices that avoid an unwanted route. Material difference alone may not describe their value.
- **Race and tempo:** both sides have threats; greedily resolving the previous exchange loses the race. Null move is particularly suspect here.
- **Repetition/progress decisions:** an apparently winning line forces a draw, or a capture/promotion resets the progress counter and changes the result. Replay the full history.
- **Safe ordinary moves:** many positions have several acceptable moves and no tactical crisis. They prevent an enriched puzzle suite from rewarding an engine that wastes time everywhere.

A practical detector can begin with the **move's changed region**: origin, all route/intermediate squares, destination, removed pieces, changed promotion capabilities, and rays that cross those squares. These identify candidate newly opened attacks and hanging pieces. Because a distant sliding route may change, “nearby” must mean dependency on those squares, not just geometric distance. Use complete legal generation as the reference when validating an incremental detector. This proposal builds on the existing [route-aware capture analysis][local-hang] and [targeted q generation][local-qgen].

For each missed tactic, replay the adjudicated move through a diagnostic search and record the first exclusion. One position can have several causes; preserve the chain instead of assigning all blame to the final q evaluation. A useful record includes root rank, reduction, remaining main depth, q-entry decision, q candidate rank, each pruning reason, TT provenance, and whether the deadline interrupted the relevant root move. Sample diagnostics separately from timing to avoid measuring instrumentation overhead.

## Normal-search options

These proposals address search allocation. They can change choices even at the same depth, so fixed-depth equality is not their strength criterion.

### S1 — Demote suspicious captures before deleting them

Replace the unconditional hang skip with late ordering or a reduced verification search for a narrow class: root/PV candidates, checking/threatening sacrifices, near-best alternatives, or moves whose post-move attack result is uncertain. Keep the existing filter for clearly futile ordinary exchanges until evidence justifies widening. A mature staged picker has a place for bad captures; “bad-looking” does not inherently mean “never search.” Cost: extra late branches; possible large tactical benefit but unmeasured. [Current skip][local-hang], [deferred bad moves][sf-picker]. **Highest priority.**

### S2 — Make LMR depend on useful evidence

Condition reductions on history, PV status, check/threat status, improving evaluation, and whether the move defends an endangered major/royal. Consider fractional reductions or a smooth depth/move-count schedule later; first test one protection at a time. At depth five, subtracting two plies is a substantial loss of tactical coverage. Do not treat a larger nominal completed depth as success if important quiet lines are more heavily reduced. [Current LMR][local-loop], [history-sensitive alternative][eth-search]. **Highest priority.**

### S3 — Add negative and continuation history

Reward successful quiet cutoffs and penalize previously searched quiets that failed, using bounded updates/aging. Add a compact context for the previous move or threat so a useful reply in one situation does not become globally favored. Compare history as ordering only before letting it control pruning. Dense chess-shaped tables can become enormous on this board; hash compact piece-class/square/route features and track collisions. [Current positive-only history][local-keys], [history implementation precedent][eth-history]. **High priority.**

### S4 — Add capture history and tactical move categories

Learn which captures produce cutoffs, conditioned on mover, victim category and route type. Combine this with safety/threat signals; raw captured material can over-rank corridor sweeps. Route multiplicity can produce many nearly identical candidates, so avoid rewarding each duplicate as independent evidence. Cost: bounded counters; modest memory if hashed. This supports both normal ordering and Q11. [Stockfish move scoring][sf-picker]. **High priority after identity audit.**

### S5 — Rework null-move eligibility

Require a credible static advantage over beta and restrict the test to suitable non-PV/cut-node contexts. Exclude threatened/low-mobility/low-material cases and positions with fragile race or royal dynamics. Compare guarded null move with the current rule and with null disabled; the latter diagnoses false cutoffs but may be too slow to ship. The current fixed reduction at shallow depth often bypasses q entirely. [Our null move][local-ab], [Berserk null conditions][berserk-search]. **Highest priority.**

### S6 — Verify risky null fail-highs

For selected shallow or tactically unstable null cutoffs, run a reduced real-move search with null disabled, or require a credible move witness. The verification costs time but can stop an optimistic pass assumption from hiding the opponent's threat. A null move is not legal evidence of a win, especially near repetition/progress boundaries. Established verified-null designs motivate the idea; their deep-search thresholds are not suitable constants for depth-five Taikyoku. [YaneuraOu null verification][yo-search]. **High priority with S5.**

### S7 — Interior PVS with consistent semantics

Scout later interior moves and fully research promising candidates, but first make PV/q horizon, TT eligibility and incoming move context explicit. The previous prototype changed several of these together and found no reliable advantage. A clean experiment would isolate the horizon policy, then TT guard, then scouts, with full-route decision adjudication. PVS remains a legitimate option; it is not the leading unexplored opportunity. [Previous PVS evidence][previous], [classical PVS precedent][sf11-search]. **Medium priority.**

### S8 — Truly staged TT-first move generation

Validate and try the entire TT route before generating all stage-A moves; then generate promising tactics, quiets, and deferred moves as needed. This can save generation even if the search tree is unchanged, but selective search and history often make ordering changes behavioral. Current stage B can delay a quiet multi-leg TT move until after stage A. The earlier TT-first prototype was mixed, so measure cutoff rank and generation work avoided before a large rewrite. [Current stages][local-ab], [earlier prototype][speed-second]. **High execution priority, conditional on semantics.**

### S9 — Persistent TT and histories across consecutive moves

Keep useful analysis within a game/evaluator session, with aging and a memory budget. Correctly bind network, configuration, rule variant and complete state identity; handle repetition/path dependence and root-relative mate scores. Separate persistent move hints from score reuse. Our previous persistent-TT work was exploratory, and allocation reuse today does not mean scores persist. Cost: ownership complexity, cache footprint and invalidation. [Prior research][speed-second], [Stockfish TT replacement][sf-tt]. **Medium priority after audit.**

### S10 — Better TT replacement and static-eval reuse

Test small clusters, depth/age/quality replacement and storing raw static evaluation separately from searched bounds. A cached static score must include evaluator/config identity; correction-history output may need recomputation. More table hits do not necessarily mean less wall time. Current single-slot replacement is simple and has no per-entry static eval. Measure useful-cutoff rate and bytes per worker. Restrict cluster counts to a layout compatible with indexing; arbitrary cluster widths require checking the power-of-two assumptions. [Our TT][local-tt], [external TT][sf-tt]. **Medium priority.**

### S11 — Shallow reverse futility and razoring

Reverse futility skips a node predicted safely above beta; razoring substitutes a qsearch when a shallow node appears far below alpha. Use generous model-calibrated margins, non-PV constraints and tactical exclusions. Our q is highly selective, so using it to certify failure can compound blind spots. These may save substantial work, but adding them before understanding current misses is premature. [Handcrafted-era precedent][sf11-search], [current q restrictions][local-q]. **Medium/later priority.**

### S12 — Late-move and history pruning

Prune sufficiently late low-history quiets at shallow nodes after enough alternatives have been searched. Protect defenses, threats, checks, relevant promotions and PV candidates. This is stronger than reduction: it can eliminate the only good quiet move. First test better ordering and LMR, then ask whether the remaining tail is expendable. Cost saving is plausible in a huge move list; strength risk is correspondingly high. [Public Ethereal move pruning][eth-search]. **Later priority.**

### S13 — ProbCut and tactical cutoffs with a witness

Search a promising tactical move against a raised beta at reduced depth, optionally preceded by q, and cut only if it clears the margin. This offers more evidence than material-only pruning, but remains probabilistic. Its benefit depends on cheap, trustworthy tactical candidates and meaningful score margins. At our shallow nominal depth the probe can consume much of the ordinary search, so count net work saved. [YaneuraOu ProbCut][yo-search]. **Medium/later priority.**

### S14 — Selective extensions and singular moves

Extend forced defenses, a unique credible saving move, difficult recaptures or a TT move proven much better than alternatives. Avoid extending every check or capture. Full singular verification can be expensive at depth five; a bounded one-defense extension is a more natural first step, and an optional version already exists locally. Protect against repeated extensions down a path. [Existing scarce-defense extension][local-leaf], [Ethereal singular search][eth-search]. **Medium priority, tactical subsets first.**

### S15 — Internal iterative reduction versus finding a good first move

Modern engines may reduce depth where no reliable TT move exists, while older designs search shallowly to obtain one. We should compare both with simply improving initial ordering. At shallow depth, taking away another ply from an already poorly understood node may worsen exactly the positions we care about. This is not an automatic missing optimization. [SF11 internal iterative deepening][sf11-search], [Berserk IIR][berserk-search]. **Low priority initially.**

### S16 — Adaptive aspiration windows and root allocation

Use recent score volatility or search instability to choose aspiration width; widen the failed side deliberately and preserve completed results. Spend verification effort on near-best alternatives with credible bound uncertainty, not only on the previous favorite. Distinguish exact root scores from fail-low bounds before comparing margins. Multipv-style full searches of every root candidate are expensive; a targeted top-candidate check is more realistic. Our previous studies did not establish this approach. [Current root windows/scouts][local-root]. **High priority for measurement, medium for implementation.**

### S17 — Time management based on decision uncertainty

Use best-move changes, score drops, root effort distribution, actual iteration growth and remaining bank to decide when more work is worthwhile. Current Fischer control uses a threefold iteration estimate and one-extra-iteration policy. A clock-aware test should replay that policy, not replace it with a fixed five-second timeout. Stockfish's root controller provides an example of stability/score/effort signals; its parameters assume a different game length and workload. [Our clock][local-clock], [Stockfish root controller][sf-time]. **High priority.**

### S18 — Correction history for systematic static-eval errors

Maintain small bounded corrections keyed by suitable position/move features, trained online from reliable search residuals. This can improve pruning and ordering without changing NNUE weights. Avoid learning from interrupted results, terminal scores or heavily selective bounds as though they were exact. Our many piece types and route effects need an economical key, not a direct copy of chess pawn/minor tables. Cost: table memory and lookup traffic; possible feedback instability. [Stockfish history structures][sf-history], [Berserk histories][berserk-history]. **Medium priority after measurement infrastructure.**

### S19 — Specialized bounded royal proof search

Use a separate forcing-line solver for last-royal attacks/defenses, returning proven win, proven refutation or unknown. Start from the existing bounded mate probe; do not call an expensive solver at every node. Verify all legal defenses, including quiets and routes, and treat time/candidate exhaustion as unknown. Ordinary NNUE scores must not masquerade as proofs. Shogi's specialized mate detection is relevant precedent, but our current probe has a deliberately narrower contract. [Local probe][local-probe], [YaneuraOu mate tests][yo-search]. **Medium priority on rare decisive positions.**

### S20 — More radical search architectures

MTD(f), best-first/beam search, selective minimax trees, learned move policies, and neural MCTS are alternatives to study if the current architecture reaches a clear limit. MTD(f) depends heavily on TT quality and repeated zero-window searches; beam methods can discard the only defense; MCTS/policy systems require a different compute/training design. They are not prerequisites for benefiting from NNUE. Treat these as longer-term research, not drop-in fixes supported by our current results. **Low immediate priority.**

## Search contracts to audit before stacking more selectivity

These observations are from current source. They identify concrete tests to write, not newly reproduced failures.

1. **Full move identity.** `MoveKey` stores endpoints and promotion but omits intermediate/free-eagle route details. The actual `Move` and GUI coordinates carry more information. TT/killer matching can therefore conflate ordering hints for distinct routes. A conflated hint is not itself a false TT score, but it matters when ordering determines pruning. Use complete canonical move identity for any new TT-first generator or history design. [Move keys][local-keys].
2. **Complete position identity.** The board already has an auxiliary movement-origin key for memoization, because promoted Whales with different origins can move differently. Main search Zobrist keys omit that origin. Audit whether TT/repetition states with identical keys can have different legal moves; do not broaden persistent score reuse until resolved. [Zobrist][local-zobrist], [documented board variant key][local-board].
3. **Incoming context on re-search.** Capture loudness, prior landing, wipe and mover size influence leaf/q entry. In the current move loop these are mutable context fields; inspect every retry path for restoration to the candidate's incoming context after a recursive search. The prior PVS prototype explicitly restored them. A stack-owned context is easier to reason about than scattered save/restore. [Move loop][local-loop], [prior report][previous].
4. **PV meaning across stages.** The move loop uses a local `i == 0` for child PV status, while LMR uses a stage-adjusted index. Audit the first move of stage B and cases where early moves were skipped, especially before introducing interior PVS. “First generated” and “first actually searched” are distinct.
5. **Bounds versus exact scores.** A scout fail-low is an upper bound, not an exact ranking against other fail-lows. A reduced/q-limited result also carries a policy assumption. Keep provenance for diagnostics and avoid using apparent score gaps as calibrated confidence.
6. **Path-dependent outcomes and termination.** Repetition is checked before TT, but checking the current node alone does not make every descendant bound independent of path history. Null moves, progress counters, extensions and added quiet q checks need explicit cycle/maximum-ply and abort behavior. A budget exhaustion result is neither proof of safety nor an exact TT entry.

The existing tactical-tree code explicitly distinguishes early search repetition from actual tournament draw adjudication; retain this distinction when classifying proofs or labels. [Worker terminal rules][local-tactical-worker].

These checks can be done with focused synthetic positions and differential references; they do not require a large tournament. Correctness fixes should be separated from strength heuristics so later results remain interpretable.

## Execution and memory options

These are candidates for preserving fixed-work behavior, except where noted. Under a time limit, even an exact speedup can change the chosen move by allowing more search.

### E1 — Avoid generating moves that a cutoff makes unnecessary

Measure how many generated/scored moves are never searched. TT-first generation, tactical-first batches, and lazily materialized quiet routes can reduce this. Preserve stable ties when claiming equivalence. Reordering in a selective engine is a behavior change even if every legal move remains available eventually. [Current staging][local-ab], [picker precedent][sf-picker].

### E2 — Share tactical classification across consumers

Compute route victims, friendly losses, promotion status and post-move landing safety once per candidate where profitable. Reuse data between ordering, pruning and make/unmake. Keep cache identities complete and avoid retaining a huge annotation object for every possible move. The earlier q annotation profile suggests a limited ceiling for annotation alone. [Prior profile][previous].

### E3 — Incremental attack/affected-ray data

Track only attacks invalidated by changed squares and movement capabilities, or cache threshold queries such as “is there any legal major capture?” Current memoization already helps; a new design must beat its warm-cache behavior. Wipes may invalidate many rays, so sparse updates need a fallback. This is a proposed structural optimization, not an established saving.

### E4 — Cheaper make/unmake and reusable move storage

Profile allocations, cloning, piece-list updates, repetition bookkeeping and undo buffers. Reuse scratch storage per ply and batch changed-piece work. Preserve the exact legal state, hashes and NNUE accumulators after undo, including promotion origins and multi-victim captures. Existing production code already restores accumulator snapshots and caches deltas; measure the remaining costs instead of repeating that work. [Prior NNUE implementation study][speed-first].

### E5 — Deferred NNUE materialization where evaluations are avoided

Track dirty features and materialize only when needed after TT/pruning decisions. Stockfish's accumulator stack demonstrates that strategy. Our earlier deferred-update screen was neutral/mixed against an already optimized baseline, so first measure the fraction of moves whose evaluation is truly avoided. Huge multi-piece deltas and a hot delta cache can change the tradeoff. [Accumulator source][sf-accum], [local deferred-update screen][speed-second].

### E6 — Cache raw evaluation selectively

Store raw NNUE outputs with complete model/position identity, then apply context-dependent corrections separately. A TT-bound hit and an eval-cache hit save different work. Our prior standalone evaluation-cache screen was mostly slower, making another unconditional cache low priority. A compact field inside an already probed TT entry may have different costs. [Previous screen][speed-second], [TT/eval separation precedent][sf-tt].

### E7 — Reduce per-worker memory without defeating locality

Tune feature-row/delta cache capacities, TT size and history layout against total concurrent workers. The earlier W512 row-plus-delta payload ceiling was 32 MiB per search thread; this is capacity, not measured extra RSS. The old W512 v4.5 confirmation process reported a 298.1–313.9 MiB high-water RSS range, which cannot identify per-variant memory deltas. Network weights, TT, scratch and process sharing are separate. [Historical RSS evidence][previous]. Benchmark cold model switches and consecutive same-model moves, and use proportional/shared memory metrics where available. [Capacity accounting and smaller-cache results][speed-first].

### E8 — Build and CPU-specific improvements

Native builds and optional PGO already have local evidence. Re-profile PGO for the changed workload; do not assume a profile trained on older shallow searches remains optimal. Check actual CPU instruction support, affinity, frequency/power and worker contention. Fat LTO, huge pages and transposed heads were not universal wins in the prior study. [Build and memory experiments][speed-second].

### E9 — Exact deduplication of equivalent child states

Different move routes may lead to the same board, but may differ in progress/repetition context or tactical metadata. Deduplicate only when the **complete search state and leaf policy** are equivalent. “Same wipe” is not sufficient if landing, promotion, surviving pieces or reply geometry differs. The current optional sibling modes are heuristic reductions, not exact equivalence proofs. [Sibling handling][local-loop].

### E10 — Parallelism matched to the objective

For a single timed game, shared-TT parallel search can trade redundant nodes for lower latency. For a tournament/training fleet, one search per core may deliver more independent games. Measure game throughput, move latency, memory and determinism separately; a larger thread count is not a free strength gain. Multi-threading also complicates history and reproducibility. **Deferred until single-thread search behavior is better understood.**

### E11 — Small-network triage or evaluation bounds

A cheap model/material screen could decide whether to invoke W512, or supply a conservative cutoff bound. Switching evaluations changes the searched function unless exact bounds are available. Training and maintaining two networks adds memory and calibration problems. Start with cost/uncertainty studies, then games; do not equate a good score regression fit with strength. The existing architecture probes already distinguish head cost from quality. [Architecture study][speed-second].

### E12 — Reduce diagnostics and timer overhead

Keep expensive tracing, unique-q tracking and detailed counters out of timing builds. Measure logging and time checks, but retain sufficiently responsive hard-stop/cancellation behavior. A batched timeout check can overrun badly when one move-generation call is unusually expensive; node count is not a fixed unit of time in this game. [Search control][local-context], [probe generation caveat][local-probe].

## What the previous measurements do and do not establish

The W512 v4.5 study used base `99f8c9b` and retained its final prototype as `6d3f30a`. Its timings are historical local observations, not fresh runs on this survey's baseline. The [report][previous], [raw summary][previous-summary] and [case-level evidence][previous-evidence] contain the backing.

- Lazy royal probing: **36 paired depth-four runs on 12 positions**, CPU-time ratio **0.9870**, exploratory game-cluster bootstrap interval **0.9542–1.0109**. Every paired search signature matched. This is a plausible small saving, not a reliable strength gain.
- Combined q preparation: ratio **1.0110**, interval **0.9876–1.0359**. Cached stage-B sorting: **0.9965**, interval **0.9782–1.0166**. Neither established a speed improvement.
- Guarded interior PVS: **96 five-second-budget searches across 16 positions, two repetitions and three arms**. Neither variant gained a completed ply; both lost a ply on the same position in both repetitions. Changed moves were not independently adjudicated. These observations do not establish an Elo loss or settle PVS generally.
- Six instrumented searches attributed **9.21%** of wall time to q generation, **2.04%** to q annotation and **0.24%** to q sorting. The sample is small and instrumentation adds overhead; global timer categories overlap local q spans. These fractions cannot be added to overlapping NNUE/make-unmake measurements.
- A battery-confounded batch was excluded. Final timed comparisons were rerun as a separately planned batch after AC restoration. Earlier small timing differences remain provisional.

Two simple calculations explain why modest gains are easy to underestimate:

1. **Amdahl ceiling.** If q sorting really consumes fraction `f=0.0024`, eliminating it entirely gives speedup `1/(1-f) ≈ 1.0024`, about 0.24%. Halving the observed q-generation fraction `0.0921` gives `1/(1-0.0921/2) ≈ 1.0483`, about 4.8% speedup. These are arithmetic illustrations using the old profile, not forecasts; actual substitutions also have overhead and may change the tree.
2. **Fractional depth.** In the simplified model `work ∝ b^depth`, a 5% speedup at effective branching factor three corresponds to `ln(1.05)/ln(3) ≈ 0.044` plies. A 10% speedup is about 0.087 plies. Neither is likely to add a whole completed ply on every test position. The model is illustrative; our real selective iteration costs vary sharply.

A small exact speedup may still help over many decisions, enable a useful verification, lower fleet cost, or improve clock safety. Conversely fewer q nodes can mean omitted defenses, and more nodes can be useful verification. There is no justified conversion from these ratios to Taikyoku Elo.

Earlier real-network work also found materially larger implementation gains, including a selected W512 v3 consecutive-position time ratio of **0.740** against that study's original main. It used different models, revisions and replayed positions; it is evidence that substantial speed opportunities have existed, not a gain still available to collect or a W512 v4.5 forecast. [Direct combined comparison][speed-second].

## A better evaluation set and measurement protocol

This is a proposed design. The counts below are planning choices, not sample sizes proven sufficient for a particular Elo threshold.

### Keep three distinct kinds of evidence

**Contract fixtures:** start with roughly 24–40 compact cases covering all mandatory evasions, terminal outcomes, repeated positions, progress resets, null context, promotion origins, route identity, PV/non-PV q mismatch and reduced re-search context. These should have exact legal/structural assertions or manually checkable forced outcomes. They protect correctness, not general strength.

**Decision suite:** assemble approximately **600 positions from at least 150 games**: 300 probability-sampled ordinary positions and 300 event-enriched positions drawn from the tactical categories above. Cap samples per game, include multiple phases and both sides to move, and preserve full replay prefixes. Split by game—and by shared starting-position family where appropriate—into development and untouched validation before tuning. The random stratum estimates typical behavior; enriched cases reveal failure modes. Report them separately unless sampling probabilities justify population weighting.

**Paired games:** after screening, compare the finalist with the fixed W512 v4.5 baseline using matched starts and swapped colors under actual deployment clocks. Include ordinary starts and selected middlegame starts, but report enriched-start results separately. Compare against a small opponent mix as well as direct A/B when practical; self-play alone can favor a style-specific exploit. Count outcomes by pair and retain draw/timeout/illegal-move categories.

Fishtest's guidance supports atomic tests, real-time validation alongside node-budget tests, and explicit statistical stopping rules; its exact chess time controls and sample sizes are not transferred here. [Test methodology][fishtest-test], [statistical framework][fishtest-math].

### Select events without assuming the recorded score is truth

Use recorded score swings, royal transitions, promotions, unusually large capture deltas, root instability and disagreements between search policies as **candidate selectors**. Review positions before and after the event, not only the obvious tactical payoff. A sudden engine score change may be a horizon correction, a bound artifact, or a timeout; it is not automatically a blunder label.

Do not recycle all sixteen previously exposed positions as a fresh holdout. Keep them as regression examples, and reserve later independent games for validation. Avoid selecting cases solely because the candidate succeeds on them. Record provenance, selection rule, model/engine hashes, full search configuration, clock state, and whether a position has already influenced tuning.

### Adjudicate decisions, including acceptable alternatives

For disagreements, force each candidate's chosen move and examine its continuation under a larger budget and less aggressive pruning. Use more than one search policy and, where available, a different strong evaluator. A deeper copy of the same selective engine can share the same blind spot. Seek an explicit refutation or forced outcome; otherwise label the case “reference preference” or “unresolved,” not solved.

Store sets of acceptable moves when several choices are close. Distinguish missing a win, turning a draw into a loss, conceding a decisive material/royal threat, and a small evaluation preference. Same-depth, same-score moves are not necessarily equally good, and different moves are not necessarily improvements.

### Measure the quantity appropriate to the change

For an **exact implementation change**, require deterministic full-route, score, bound/root-line and node signatures on completed fixed-work searches, then paired timing on representative hardware. Report equal-position ratios and total-work ratios, cold versus warm behavior, variability and peak/steady memory. Tiny exact optimizations need a reproducible time saving; they do not need a whole-ply gain to be useful.

For a **search-policy change**, use equal wall-clock budgets with the same trained net and hardware. Report critical-move recall, adjudicated severe-error counts, unresolved cases, time-to-find-and-retain a sound move, deadline overruns, and game results. Also report completed depth, q share, search rate and retries as diagnostics. Run a secondary fixed-node comparison to separate allocation quality from implementation speed; it cannot replace real-time games.

Use several relevant budgets—provisionally one, five and fifteen seconds for a diagnostic sweep—plus the actual Fischer clock/remainder distribution. Those values are proposed anchors, not a claim about current fleet controls. Include the cost of the real move-selection path and expensive setup where deployment pays it. Keep per-search isolated timing as a separate microbenchmark.

### Record work distribution, not only totals

Useful counters include:

- q entry/skip counts by trigger, and reference-discovered tactics in skipped leaves;
- candidates generated, eligible, searched and pruned by reason, stratified by main depth, PV status and move class;
- rank of critical moves, first-move cutoff rate, quiet-defense reductions and reduced-search retry success;
- null attempts, fail-highs, verifications and independently refuted cutoffs;
- TT move-only hits versus score cutoffs, policy mismatch and replacement collisions;
- evaluation/materialization calls, changed-piece count, cache hits, allocations and generation avoided;
- root effort distribution, iteration growth, unused bank, hard aborts and completed-result fallback.

Use game-cluster uncertainty estimates for position suites. Timing repetitions are not independent new positions. Randomize/interleave paired runs, control power/thermal state and concurrent load, and retain failures. Predeclare the primary metric and stopping rule; if many variants are explored, treat the winner as selected on development data and confirm it independently. Fishtest documents both selection bias and the difference between passing-patch estimates and cumulative progress. [Testing caveats][fishtest-faq].

## Recommended order of work

### First: understand exclusions and protect contracts

Extend the existing tactical-tree worker with exclusion counters and a separately configured broader-search reference; add targeted tests for the six audit items. Build a small new game-disjoint pilot set before touching defaults. Deliverable: concrete examples of missed moves and their first exclusion point. This has the highest information value because it tells us which of the many plausible ideas addresses an actual weakness.

### Then: three narrow behavioral experiments

1. **Sacrifice verification:** S1 on root/PV or explicitly threatening captures. Primary question: does it recover sound tactical moves without excessive time loss? Track how often the old hang rule is refuted.
2. **Threat-aware q:** Q2/Q3 on a single well-defined threat class, with Q1 as a diagnostic comparator. Primary question: does it repair quiet-frontier and small-defender blind spots while ordinary-position cost stays acceptable? Do not bundle all q extensions.
3. **Quiet-defense LMR:** S2 using a cheap defense/threat signal, optionally preceded by S3 ordering-only work. Primary question: does the necessary quiet reply survive sufficient depth at the same clock budget?

Test null eligibility/verification as the next independent branch, or sooner if the diagnostics show null cutoffs causing the misses. Keep all search trials on W512 v4.5 first; the relevant model has been verified by descriptor hash rather than inferred from a display name.

### In parallel as a research track, not simultaneous timing jobs

Profile current generation/state-update cost and try a full-route TT-first picker or shared tactical metadata only if the avoided-work counters justify it. Keep old exact-speed candidates small and evaluate them at deployment concurrency. Revisit PVS only with isolated horizon/TT/context changes. Retire decided experimental paths instead of accumulating toggles, and follow the repository's history-freeze rule when a major search behavior change is eventually merged.

### Adoption criteria

- **Correctness fixes:** reproducible failing contract/position, targeted regression coverage and normal validation.
- **Exact speedups:** exact completed-work signatures plus a reproducible useful time/memory improvement on relevant workloads.
- **Search behavior:** improved decision evidence with no unacceptable severe-error/clock regression, followed by an adequately designed paired-game confirmation. If the result is statistically unresolved, call it unresolved.

The current evidence supports a research order, not a claim that any proposed option has already made the agent stronger.

## Scope and reproducibility

The source census inspected six external snapshots: modern Stockfish, Stockfish 11, YaneuraOu, Pikafish, Berserk and public Ethereal. That is five engine families/implementations plus a historical version, with shared ancestry explicitly acknowledged. It is not an exhaustive survey of every engine or every game.

[sources.json](sources.json) records external commits, reviewed file hashes, links, local baseline and the W512 v4.5 descriptor identity. External code was read from public GitHub raw files; no external engine was built or run. The report reproduces no source-file copies under `benchmarks/`. Old experiment results remain in their original revision with links. No new performance or Elo data were collected.

[VALIDATION.md](VALIDATION.md) records repository checks and document checks. The companion interactive report provides an option index and comparisons; the prose here is the complete report.

## Source index

Links in the report point to immutable source revisions where possible. Mechanism references do not supply performance estimates for Taikyoku. Public documentation was accessed on 25 October 2026.

- [stockfish — `dbe600b74386`](https://github.com/official-stockfish/Stockfish/tree/dbe600b7438662e9ade60b31fa0625259dcd0077); commit timestamp `2026-10-24T18:14:41Z`.
- [stockfish11 — `c3483fa9a7d7`](https://github.com/official-stockfish/Stockfish/tree/c3483fa9a7d7c0ffa9fcc32b467ca844cfb63790); commit timestamp `2020-01-18T00:44:37Z`.
- [yaneuraou — `0f421391ae31`](https://github.com/yaneurao/YaneuraOu/tree/0f421391ae311df9df57dcfcb2b4a78a1972bdf8); commit timestamp `2026-10-25T06:44:15Z`.
- [pikafish — `5ca32d8a5050`](https://github.com/official-pikafish/Pikafish/tree/5ca32d8a50506da70c7319fc228e90c0c6b5dea0); commit timestamp `2026-10-25T15:12:47Z`.
- [ethereal — `0e47e9b67f34`](https://github.com/AndyGrant/Ethereal/tree/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a); commit timestamp `2024-06-11T19:50:35Z`.
- [berserk — `32628515050b`](https://github.com/jhonnold/berserk/tree/32628515050b83805bab4afa1026dd2bcaa93f55); commit timestamp `2026-09-07T03:48:14Z`.

[local-ab]: https://github.com/yetiowin805/taikyoku_shogi/blob/3a2044085acb4c4c7828b1812b8052fe917b7b04/src/search.rs#L3396-L3600
[local-root]: https://github.com/yetiowin805/taikyoku_shogi/blob/3a2044085acb4c4c7828b1812b8052fe917b7b04/src/search.rs#L2398-L2714
[local-loop]: https://github.com/yetiowin805/taikyoku_shogi/blob/3a2044085acb4c4c7828b1812b8052fe917b7b04/src/search.rs#L3614-L3810
[local-leaf]: https://github.com/yetiowin805/taikyoku_shogi/blob/3a2044085acb4c4c7828b1812b8052fe917b7b04/src/search.rs#L3803-L3935
[local-q]: https://github.com/yetiowin805/taikyoku_shogi/blob/3a2044085acb4c4c7828b1812b8052fe917b7b04/src/search.rs#L3975-L4495
[local-qgen]: https://github.com/yetiowin805/taikyoku_shogi/blob/3a2044085acb4c4c7828b1812b8052fe917b7b04/src/search.rs#L1664-L1825
[local-keys]: https://github.com/yetiowin805/taikyoku_shogi/blob/3a2044085acb4c4c7828b1812b8052fe917b7b04/src/search.rs#L640-L729
[local-tt]: https://github.com/yetiowin805/taikyoku_shogi/blob/3a2044085acb4c4c7828b1812b8052fe917b7b04/src/search.rs#L529-L637
[local-context]: https://github.com/yetiowin805/taikyoku_shogi/blob/3a2044085acb4c4c7828b1812b8052fe917b7b04/src/search.rs#L2232-L2398
[local-hang]: https://github.com/yetiowin805/taikyoku_shogi/blob/3a2044085acb4c4c7828b1812b8052fe917b7b04/src/search.rs#L1024-L1076
[local-tactical-doc]: https://github.com/yetiowin805/taikyoku_shogi/blob/3a2044085acb4c4c7828b1812b8052fe917b7b04/training/tactical_tree/README.md
[local-tactical-worker]: https://github.com/yetiowin805/taikyoku_shogi/blob/3a2044085acb4c4c7828b1812b8052fe917b7b04/src/bin/tactical_tree_worker.rs#L1-L168
[yo-movegen]: https://github.com/yaneurao/YaneuraOu/blob/0f421391ae311df9df57dcfcb2b4a78a1972bdf8/source/movegen.h
[local-zobrist]: https://github.com/yetiowin805/taikyoku_shogi/blob/3a2044085acb4c4c7828b1812b8052fe917b7b04/src/zobrist.rs#L24-L71
[local-board]: https://github.com/yetiowin805/taikyoku_shogi/blob/3a2044085acb4c4c7828b1812b8052fe917b7b04/src/board.rs#L287-L302
[local-clock]: https://github.com/yetiowin805/taikyoku_shogi/blob/3a2044085acb4c4c7828b1812b8052fe917b7b04/src/training/clock.rs#L1-L94
[local-probe]: https://github.com/yetiowin805/taikyoku_shogi/blob/3a2044085acb4c4c7828b1812b8052fe917b7b04/src/eval/royal_probes.rs#L132-L245
[speed-first]: https://github.com/yetiowin805/taikyoku_shogi/blob/3a2044085acb4c4c7828b1812b8052fe917b7b04/benchmarks/nnue_followup/README.md
[speed-second]: https://github.com/yetiowin805/taikyoku_shogi/blob/3a2044085acb4c4c7828b1812b8052fe917b7b04/benchmarks/nnue_followup/COMPREHENSIVE.md
[previous]: https://github.com/yetiowin805/taikyoku_shogi/blob/6d3f30ae80036394d0ce4bf158fec69b800bee6d/benchmarks/nnue_search_efficiency/README.md
[previous-summary]: https://github.com/yetiowin805/taikyoku_shogi/blob/6d3f30ae80036394d0ce4bf158fec69b800bee6d/benchmarks/nnue_search_efficiency/results/summary.json
[previous-evidence]: https://github.com/yetiowin805/taikyoku_shogi/blob/6d3f30ae80036394d0ce4bf158fec69b800bee6d/benchmarks/nnue_search_efficiency/results/report-evidence.json
[sf-search]: https://github.com/official-stockfish/Stockfish/blob/dbe600b7438662e9ade60b31fa0625259dcd0077/src/search.cpp#L735-L1674
[sf-q]: https://github.com/official-stockfish/Stockfish/blob/dbe600b7438662e9ade60b31fa0625259dcd0077/src/search.cpp#L1686-L1912
[sf-time]: https://github.com/official-stockfish/Stockfish/blob/dbe600b7438662e9ade60b31fa0625259dcd0077/src/search.cpp#L561-L640
[sf-picker]: https://github.com/official-stockfish/Stockfish/blob/dbe600b7438662e9ade60b31fa0625259dcd0077/src/movepick.cpp#L188-L355
[sf-history]: https://github.com/official-stockfish/Stockfish/blob/dbe600b7438662e9ade60b31fa0625259dcd0077/src/history.h
[sf-tt]: https://github.com/official-stockfish/Stockfish/blob/dbe600b7438662e9ade60b31fa0625259dcd0077/src/tt.cpp
[sf-eval]: https://github.com/official-stockfish/Stockfish/blob/dbe600b7438662e9ade60b31fa0625259dcd0077/src/evaluate.cpp#L52-L87
[sf-accum]: https://github.com/official-stockfish/Stockfish/blob/dbe600b7438662e9ade60b31fa0625259dcd0077/src/nnue/nnue_accumulator.cpp#L76-L232
[sf11-search]: https://github.com/official-stockfish/Stockfish/blob/c3483fa9a7d7c0ffa9fcc32b467ca844cfb63790/src/search.cpp
[sf11-picker]: https://github.com/official-stockfish/Stockfish/blob/c3483fa9a7d7c0ffa9fcc32b467ca844cfb63790/src/movepick.cpp#L246-L266
[yo-search]: https://github.com/yaneurao/YaneuraOu/blob/0f421391ae311df9df57dcfcb2b4a78a1972bdf8/source/engine/yaneuraou-engine/yaneuraou-search.cpp#L2804-L4060
[yo-q]: https://github.com/yaneurao/YaneuraOu/blob/0f421391ae311df9df57dcfcb2b4a78a1972bdf8/source/engine/yaneuraou-engine/yaneuraou-search.cpp#L4441-L5148
[yo-picker]: https://github.com/yaneurao/YaneuraOu/blob/0f421391ae311df9df57dcfcb2b4a78a1972bdf8/source/movepick.cpp
[pika-q]: https://github.com/official-pikafish/Pikafish/blob/5ca32d8a50506da70c7319fc228e90c0c6b5dea0/src/search.cpp#L1592-L1820
[pika-position]: https://github.com/official-pikafish/Pikafish/blob/5ca32d8a50506da70c7319fc228e90c0c6b5dea0/src/position.cpp
[berserk-search]: https://github.com/jhonnold/berserk/blob/32628515050b83805bab4afa1026dd2bcaa93f55/src/search.c
[berserk-picker]: https://github.com/jhonnold/berserk/blob/32628515050b83805bab4afa1026dd2bcaa93f55/src/movepick.c#L198-L280
[berserk-history]: https://github.com/jhonnold/berserk/blob/32628515050b83805bab4afa1026dd2bcaa93f55/src/history.c
[eth-search]: https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/search.c
[eth-picker]: https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/movepicker.c
[eth-history]: https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/history.c
[eth-eval]: https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/evaluate.c#L440-L470
[nnue-doc]: https://official-stockfish.github.io/docs/nnue-pytorch-wiki/docs/nnue.html
[fishtest-test]: https://official-stockfish.github.io/docs/fishtest-wiki/Creating-my-first-test.html
[fishtest-math]: https://official-stockfish.github.io/docs/fishtest-wiki/Fishtest-Mathematics.html
[fishtest-faq]: https://official-stockfish.github.io/docs/fishtest-wiki/Fishtest-FAQ.html
