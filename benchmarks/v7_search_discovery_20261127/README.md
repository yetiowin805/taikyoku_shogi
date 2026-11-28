# Finding new search variants in v7 tournament games

The first two ideas to investigate are **giving quiet moves from different pieces an earlier root search** and **checking the recapture that settles a line-clearing sacrifice**. The same games suggest a clock experiment. Quiet moves that enable a teammate and expensive searches that rarely affect a decision are two places to mine for further ideas. Each proposal below has a specific observation that would justify putting it in the next tournament wave.

## What the pilot measured

At 10,266 completed games on 27 November 2026, we sampled 24 decisions each from `256_v7_S3_E_HLF_G3ATK` and `256_v7_S3_Ao_HLF_HR_G3ATK`, all at 1+1. Each decision came from a different opening pair. For each agent, half came from its own turns 15–50 and half from turns 51–150. Eligible positions had an absolute static evaluation of at most 1,500 engine units and reached depth three in the game. We replayed depths one and two with the recorded network, history and engine build. An opening sample followed the same two agents for their first 24 decisions in 16 games. One game appeared in both samples. The accompanying data includes the selection rules, slots, one-based plies, full move routes, review scores and build hashes. [Pilot data](pilot.txt#L3-L14) · [All 48 root decisions](pilot.txt#L53-L102) · [Build provenance](pilot.txt#L132-L137)

## 1. Search quiet moves from different pieces earlier

Depth three changed the depth-two choice in **26 of 48** positions. Four returned to the depth-one choice. Of the 22 moves that first became best at depth three, 19 were quiet, 19 started from a different piece than both earlier winners, and **17 met both conditions**. Quiet here means no promotion and no pieces of either side removed. These boards had a median of 691.5 legal routes. On ten of the 22 disagreements, a third network reviewed both forced moves at the same depth and preferred the deeper choice six times. [Move changes and independent reviews](pilot.txt#L9-L25)

**What to mine.** Recover the depth-two rank and depth-three search order of each eventual winner. Count how many alternatives from the same piece the engine tried first. Compare with ordinary quiet moves on the same board and include turns where the clock interrupted depth three. A concrete case is slot 9035, ply 115. At depth two Black preferred a Bishop General sweep that removed eight friendly pieces. At depth three Black played a quiet Great Master move. Two other networks preferred the Great Master at three plies after each forced move. Both found the Flying Eagle capture against the Bishop General. [Slot 9035 review](pilot.txt#L27-L32)

**Candidate.** Keep the previous best move first. Try the highest-ranked quiet move from each of two other pieces before returning to the usual order. Preserve each full route. Compare ordering alone and ordering with the existing partial-root rule, which can use a verified improvement when time expires. Advance this if worthwhile challengers repeatedly arrive late enough for reordering to matter. [Root ordering and partial-root verification](../../src/search.rs#L3475-L3515)

## 2. Verify what a line-clearing sacrifice actually buys

The opening sample contained **66 distinct deployments** where a range capturer removed at least two friendly pieces and no enemies. In 12, the piece captured enemy material on its next turn and the opponent immediately took it on its landing square. In slot 9290, White's evaluation four plies after its Fierce Dragon deployment at ply 2 had risen **1,832** engine units. After its Vice General deployment at ply 18 it had fallen **1,700**. These scores include the intervening play. The first sequence is a useful control for any rule that changes the second. [Opening counts, captures and evaluations](pilot.txt#L34-L51)

**What to mine.** Replay the 12 exchanges and inspect the search from before deployment. Was the recapture generated, pruned, or searched? At what remaining depth and with what score? Force the best recapture and compare the original choice with a plausible alternative. Include deployments where the sweeper survived, and the known final-quiescence-ply case from slot 7773. [Slot 7773 trace and earlier recapture proposals](../../FUTURE_NOTES.md#quiescence-can-stop-one-move-before-a-major-recapture)

**Candidate.** When a shallow line justifies a friendly clearance by the deploying piece's next capture, verify the opponent's recapture before accepting that gain. Reuse the existing recapture search wherever it already covers the reply. If a fully searched reply leaves the same misvaluation, investigate how the network values the resulting position. [Existing quiescence extension](../../src/search.rs#L4298-L4344)

## 3. Spend a little more clock where another iteration may matter

Depth three changed the move in **3 of 14** positions where the depth-two choice took an enemy, and **23 of 34** where it took none. The latter group includes friendly clearances. An earlier census of the same agents found 25,563 of 189,364 turns at 1+1 finished below depth three. Of those, 19,910 started depth three and ran out of time during the iteration, while 5,653 never started it. These two cases offer different opportunities for clock allocation and partial-root search. [Pilot counts](pilot.txt#L9-L11) · [Clock census by phase and agent](pilot.txt#L104-L113)

**Candidate.** When the existing clock policy is close to admitting a third iteration, test a modest bank allowance if the incumbent took no enemy. Keep a reserve for later turns. Sample the interrupted and unattempted cases separately and match controls on game phase and remaining bank. Measure how often the extra time changes a reviewed decision for the better, what it costs, and whether the agent subsequently flags or loses depth. [Iteration budget](../../src/training/clock.rs#L57-L92)

## 4. Work backward from a threat to the teammate that enabled it

A quiet move may matter because it opens a route for another piece several plies later. Start with the first credible threat by a previously boxed-in major in a competitive position. Rewind to the teammate move that cleared the route. Force an ordinary alternative at that earlier position and check whether the threat is still available against a reasonable defense. Count comparable opportunities where opening a route accomplished little. This strategic idea still needs examples.

**Candidate.** If useful unblocks share a cheap, identifiable feature, reduce those quiet moves one ply less during late-move reductions. For example, test a newly opened route to a contested target. Record what this finds beyond the existing immediate major-threat detector and how often the trigger fires in ordinary play. [Major-threat detection](../../src/search/experiments.rs#L419-L448)

## 5. Find cases where an expensive search can be narrowed

In eight color-reversed pairs, `256_v7_S3_E_HLF_G3ATK` and its `ATKPVSPV` counterpart reached depth three on 73 distinct shared opening histories. The median ratio of `ATKPVSPV` time to `ATK` time through depth three was **1.012**, with the middle half from **0.976 to 1.057**. Individual positions varied more. Those timings identify positions worth inspecting. They cannot attribute the time to a particular search operation. [Timing method and slot pairs](pilot.txt#L115-L130)

**What to mine.** For one expensive extension or re-search, trace the nodes where it fires. Compare skipping just that invocation on the same position. Record the time, whether the returned bound or root choice changes, and the rare cases where verification saves a good move. If a cheap condition preserves those saves on fresh positions, try a narrower version and measure what the released time buys elsewhere at 1+1.

## 6. Follow an existing threat through a sacrifice elsewhere

**Replay slot 10358 from before ply 1135** in the corrected-rules tournament. Black was `256_v7_S3_Ao_HLF_G3PR` and White was `256_v6Cm_HLFS_G3QH`, playing 15+5. Black’s Water Dragon forked two Tengus. At 1136 White swept three pieces with its Great General. If Black immediately took either Tengu, the General could sweep five more Black pieces, including the Great Dove and Rain Demon. Instead, at 1137 the Great Dove took the General first, leaving the fork intact. At 1138 White took a Lion with one Tengu. Black took the other Tengu at 1139, as White’s search expected, and the survivor took a Rear Standard at 1140. The decision to investigate is the General sacrifice at **1136**. The Lion capture at **1138** is a useful control where the agent already saw the coming loss.

With White’s regular search fixed at depth two, varying only the maximum quiescence depth reproduced the played move at the tournament setting:

| Maximum quiescence plies | White’s choice at 1136 | Quiescence nodes |
|---:|---|---:|
| 1 | Tengu captures the Water Dragon | 9,009 |
| 2 (tournament) | Great General sacrifice | 374,893 |
| 3 | Tengu escapes and captures a Turtledove | 375,151 |
| 4 | Same Tengu escape | 374,554 |

The sacrifice led the immediate Water Dragon capture by 110 White evaluation points with one regular reply ply after each forced move. It trailed by 2,313 with two reply plies. At four reply plies it led again by 332. These are matched comparisons using White’s evaluator and the normal quiescence policy. Use this slot as evidence of sensitivity to depth. The objectively best move is still open, and the 258-node cost of the third quiescence ply is a single-position observation.

**Experiments to separate:**

1. Raise the maximum quiescence depth from two to three while retaining the existing one-ply cap on most non-principal lines. Measure quiescence nodes, time, and regular depths completed at 1+1. [Leaf budget policy](../../src/search.rs#L4155-L4162)
2. Remember a fork of two valuable pieces for one intervening exchange. Before accepting a static score, verify whether the original attacker still has a legal capture on either target. If it does, give the defender a bounded search including quiet escapes, captures of the attacker and genuine counterthreats. Trace whether the decisive reply was excluded by the last-landing-square filter, exhausted its budget, or was searched and evaluated favorably. The pilot did not isolate which happened here. [Quiescence capture selection](../../src/search.rs#L4442-L4483)
3. When an immediately capturable sacrifice becomes best in a shallow root iteration, give it and the displaced incumbent the same extra reply budget. Test whether the opponent can take the sacrifice first and collect the earlier target later. As a separate exploratory trigger, a hypothetical pass can identify a pending major loss and nominate real defenses to search. Use the pass only to identify a threat. [Beal’s generalised quiescence search](https://www.sciencedirect.com/science/article/pii/0004370290900728)

**How to mine more games.** Start at positions where a newly attacked major remains exposed after its owner makes a forcing move elsewhere. Track the original attacker and victim across the interruption, including targets captured along a route. Replay both reply orders from the same pre-sacrifice position. Record what each side could collect, the first depth at which the preferred order changes, and whether the agent’s root choice follows it. Include ordinary exposed majors and sound counterthreats where taking the original target first really does lose material. Count opportunities across held-out opening pairs and measure how often verification saves a decision per millisecond at the tournament clock.

## Choosing the next experiment set

1. **Count the opportunities.** Include ordinary decisions where each proposed trigger was available. Keep both colors of an opening pair together and hold out whole opening families for the final check. Use a small 15+5 sample to see which continuations survive more time. Compare speed under the clock where the candidate will play. Professional shogi realization-probability search used the frequency of a move category *when available* to allocate depth. Work on Breakthrough also showed how to discover extension triggers by combining simple move features. [Shogi game-record method](https://www.nactem.ac.uk/tsuruoka/papers/icga02.pdf) · [Search-extension feature discovery](https://staff.ru.is/yngvi/pdf/SkowronskiBW09.pdf)
2. **Keep a replayable case for and against each rule.** Start at the position before the specified ply with the original history, network, build and clock. Compare the played move with the candidate on the same board. Record the reply, score from the same side's perspective, completed depth, nodes and milliseconds. Give priority to differences that change a competitive decision. [Position analyzer](../../src/bin/analyze_position.rs) · [Tactical replay worker](../../training/tactical_tree/README.md#protocol-durability-and-cpu-ownership)
3. **Choose mechanisms before multiplying them across models.** Take forward candidates that fix a recognizable failure or save time while preserving important replies. Retain single-change controls for combinations, particularly root ordering with partial-root acceptance or the clock change. Deduplicate against existing configurations, then distribute promising variants across the leading v7 checkpoints toward the roughly 100-agent wave. Tournament games decide whether the tradeoff improves playing strength.
