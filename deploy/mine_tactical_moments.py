#!/usr/bin/env python3
"""Mine score-drop episodes; these are candidates, never automatically solved puzzles."""

import argparse
import collections
import hashlib
import html
import json
import statistics
import subprocess
from pathlib import Path

MATE_BAND = 100_000


def own_rows(game, color):
    sign = 1 if color == "Black" else -1
    clock = ((game.get("stats") or {}).get("clock") or {}).get("moves", [])
    rows = []
    for i, move in enumerate(game["moves"]):
        if move["color"] != color:
            continue
        score = move.get("eval")
        rows.append(
            dict(
                ply=i + 1,
                score=None if score is None else sign * score,
                static=None if move.get("static_eval") is None else sign * move["static_eval"],
                depth=move.get("completed_depth"),
                nodes=move.get("nodes"),
                elapsed_ms=clock[i].get("elapsed_ms") if i < len(clock) else None,
                remaining_ms=clock[i].get("remaining_ms") if i < len(clock) else None,
                record_move_number=move["move_number"],
            )
        )
    return rows


def finite(row):
    return row["score"] is not None and abs(row["score"]) < MATE_BAND and row["depth"] != 0


def candidates(rows, threshold=1200, radius=6):
    """Compare only contiguous own decisions; never bridge missing/mate/fallback rows."""
    raw = []
    for j, low in enumerate(rows):
        if not finite(low):
            continue
        for span in range(1, 4):
            i = j - span
            if i < 0 or not all(finite(r) for r in rows[i : j + 1]):
                continue
            high = rows[i]
            drop = high["score"] - low["score"]
            if drop < threshold:
                continue
            future = []
            for following in rows[j : j + 3]:
                if not finite(following):
                    break
                future.append(following["score"])
            sustained = len(future) >= 2 and statistics.median(future) <= high["score"] - drop / 2
            crossing = high["score"] >= 0 and low["score"] < 0
            comparable = next(
                (
                    r
                    for r in reversed(rows[max(0, j - 6) : j])
                    if finite(r) and low["depth"] is not None and r["depth"] == low["depth"]
                ),
                None,
            )
            same_depth_drop = comparable["score"] - low["score"] if comparable else None
            # Prefer short, persistent losses of a plausible position, without deleting
            # oscillations or already-lost positions from the candidate pool.
            priority = drop * (1.25 if sustained else 0.65) * (1.2 if crossing else 1)
            if high["score"] < -3000:
                priority *= 0.6
            if span > 1:
                priority *= 0.9
            raw.append(
                dict(
                    high=high,
                    low=low,
                    own_turn_span=span,
                    drop=drop,
                    sustained=sustained,
                    crosses_zero=crossing,
                    priority=round(priority, 1),
                    same_depth_previous_ply=comparable["ply"] if comparable else None,
                    same_depth_drop=same_depth_drop,
                    depth_change=None
                    if high["depth"] is None or low["depth"] is None
                    else low["depth"] - high["depth"],
                )
            )
    chosen = []
    for c in sorted(raw, key=lambda c: (-c["priority"], c["low"]["ply"])):
        # Local nonmaximum suppression; context windows may overlap, but nearby
        # recognition points are one episode rather than independent test cases.
        if all(abs(c["low"]["ply"] - p["low"]["ply"]) > radius for p in chosen):
            chosen.append(c)
    return sorted(chosen, key=lambda c: c["low"]["ply"]), len(raw)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def build(args):
    inventory = json.loads(args.inventory.read_text())
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    (output / "replays").mkdir(exist_ok=True)
    cases, games, positions, trajectories = [], [], {}, {}
    raw_count = 0
    for g in inventory["losses"]:
        path = (args.games / Path(g["path"]).name).resolve()
        game = json.loads(path.read_text())
        expected = "WhiteWins" if g["agent_color"] == "Black" else "BlackWins"
        if game["result"] != expected:
            raise ValueError(f"slot {g['slot']}: manifest/result mismatch")
        rows = own_rows(game, g["agent_color"])
        episodes, n = candidates(rows, args.threshold)
        raw_count += n
        # Preserve a stable non-drop control per game, to discourage tests that
        # reward changing every move or spending unbounded extra time.
        stable = [
            r
            for i, r in enumerate(rows[1:-2], 1)
            if finite(r)
            and finite(rows[i - 1])
            and finite(rows[i + 1])
            and abs(r["score"] - rows[i - 1]["score"]) < 150
            and abs(r["score"] - rows[i + 1]["score"]) < 150
            and all(abs(r["ply"] - e["low"]["ply"]) > 12 for e in episodes)
        ]
        control = (
            min(stable, key=lambda r: hashlib.sha256(f"{g['slot']}:{r['ply']}".encode()).hexdigest())
            if stable
            else None
        )
        wanted = {r["ply"] for e in episodes for r in rows if e["high"]["ply"] - 4 <= r["ply"] <= e["low"]["ply"] + 4}
        if control:
            wanted.add(control["ply"])
        replay_path = output / "replays" / f"slot{g['slot']:04}.jsonl"
        with replay_path.open("w") as stream:
            subprocess.run([str(args.replay), str(path), ",".join(map(str, sorted(wanted)))], stdout=stream, check=True)
        events = [json.loads(line) for line in replay_path.read_text().splitlines()]
        if len(events) != len(game["moves"]):
            raise ValueError(f"slot {g['slot']}: incomplete replay")
        by_ply = {r["ply"]: r for r in rows}
        build = game[g["agent_color"].lower()].get("engine_build", {})
        for event in events:
            tsfen = event.pop("position_before")
            key = event.pop("position_hash_before")
            if tsfen:
                positions.setdefault(key, dict(id=key, tsfen=tsfen, occurrences=[]))["occurrences"].append(
                    dict(slot=g["slot"], ply=event["ply"], agent=g["agent"], game=str(path))
                )
                event["position_id"] = key
        # Keep the bulky position text only in the shared position store.
        replay_path.write_text("".join(json.dumps(event) + "\n" for event in events))

        def snapshot(ply, events=events, by_ply=by_ply):
            return dict(events[ply - 1], **{k: v for k, v in by_ply.get(ply, {}).items() if k != "ply"})

        for e in episodes:
            start = max(1, e["high"]["ply"] - 4)
            end = min(len(events), e["low"]["ply"] + 12)
            anchors = [snapshot(p) for p in sorted(wanted) if start <= p <= e["low"]["ply"] + 4]
            sign = 1 if g["agent_color"] == "Black" else -1
            delta = sign * (
                events[end - 1]["material_black_after"] - events[e["high"]["ply"] - 1]["material_black_before"]
            )
            flags = []
            if not e["sustained"]:
                flags.append("rebounds_or_short_followup")
            if e["depth_change"] and e["depth_change"] < 0:
                flags.append("recognition_depth_lower")
            if e["depth_change"] and (e["same_depth_drop"] is None or e["same_depth_drop"] < args.threshold):
                flags.append("depth_confounded_drop")
            if e["high"]["score"] < -3000:
                flags.append("already_losing")
            if end == len(events):
                flags.append("near_game_end")
            major_losses = [
                dict(ply=e["ply"], attacker=e["piece"], victim=p["piece"], seed_value=p["seed_value"])
                for e in events[start - 1 : end]
                if e["color"] != g["agent_color"]
                for p in e["removed"]
                if p["color"] == g["agent_color"] and p["seed_value"] >= 648
            ]
            category = (
                "major_piece_loss_in_context"
                if major_losses
                else "other_material_collapse"
                if delta < -500
                else "score_instability_or_deferred_tactic"
            )
            cases.append(
                dict(
                    id=f"s{g['slot']:04}-p{e['low']['ply']:04}",
                    slot=g["slot"],
                    agent=g["agent"],
                    opponent=g["opponent"],
                    color=g["agent_color"],
                    game=str(path),
                    game_sha256=digest(path),
                    engine_build=build,
                    model=game[g["agent_color"].lower()].get("model"),
                    status="unvalidated_candidate",
                    **e,
                    flags=flags,
                    anchor_positions=anchors,
                    context=events[start - 1 : end],
                    material_delta_through_context=round(delta, 1),
                    category=category,
                    major_losses=major_losses,
                )
            )
        games.append(
            dict(
                **g,
                local_path=str(path),
                sha256=digest(path),
                plies=len(events),
                candidates=len(episodes),
                control=snapshot(control["ply"]) if control else None,
                excluded_score_rows=sum(not finite(r) for r in rows),
                engine_build=build,
            )
        )
        trajectories[str(g["slot"])] = rows
    # Build connected components of games sharing any selected exact position.
    # Entire components stay together: splitting only by game would leak shared openings.
    parent = {g["slot"]: g["slot"] for g in games}

    def find(x):
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    for pos in positions.values():
        slots = [x["slot"] for x in pos["occurrences"]]
        for slot in slots[1:]:
            parent[find(slot)] = find(slots[0])
    components = collections.defaultdict(list)
    for g in games:
        components[find(g["slot"])].append(g["slot"])
    ordered = sorted(components.values(), key=lambda s: hashlib.sha256(str(sorted(s)).encode()).hexdigest())
    # Previously investigated games stay in development; reserve whole novel groups.
    reserved = set()
    for group in ordered:
        if not ({673, 817} & set(group)) and len(reserved) + len(group) <= max(1, len(games) // 4):
            reserved.update(group)
    seen = {}
    for case in sorted(cases, key=lambda c: -c["priority"]):
        anchor = next(p for p in case["anchor_positions"] if p["ply"] == case["high"]["ply"])
        key = (case["agent"], anchor["position_id"])
        case["duplicate_of"] = seen.get(key)
        seen.setdefault(key, case["id"])
        case["split"] = "holdout" if case["slot"] in reserved else "development"
    for g in games:
        g["split"] = "holdout" if g["slot"] in reserved else "development"
    corpus = dict(
        schema_version=1,
        captured=inventory["captured"],
        completed_games=inventory["completed_games"],
        scope="All v4/v4.5 losses to handcrafted or v2 in " + inventory["run_id"],
        threshold=args.threshold,
        score_convention="Own perspective, internal engine score units; not calibrated centipawns",
        ply_convention="1-based GUI replay ply; position is BEFORE that move; record_move_number is separate",
        raw_windows=raw_count,
        games=games,
        cases=sorted(cases, key=lambda c: -c["priority"]),
        unique_cases=len(seen),
        holdout_slots=sorted(reserved),
        limitations=[
            "Score drops are discovery signals, not ground-truth tactics.",
            "Seed material deltas are descriptive, not learned evaluation.",
            "TSFEN snapshots omit repetition history; use the source replay for search.",
            "Observed continuation need not be best play; avoidance moves require adjudication.",
        ],
    )
    (output / "corpus.json").write_text(json.dumps(corpus, indent=2) + "\n")
    (output / "positions.jsonl").write_text("".join(json.dumps(p) + "\n" for p in positions.values()))
    (output / "trajectories.json").write_text(json.dumps(trajectories) + "\n")
    write_review(corpus, output)
    return corpus


def write_review(corpus, output):
    rows = []

    def short(s):
        return s.replace("NNUE_W", "").replace("_v", "v").replace("BASE_C2S2_", "")

    for c in corpus["cases"]:
        label = f"{c['id']} · {short(c['agent'])} vs {short(c['opponent'])} · {c['high']['score']:+} → {c['low']['score']:+}"
        detail = f"Decision ply {c['high']['ply']}; recognition ply {c['low']['ply']}; drop {c['drop']}; {c['split']}; "
        detail += ", ".join(c["flags"]) or "sustained"
        detail += "; " + c["category"].replace("_", " ")
        if c["duplicate_of"]:
            detail += "; duplicate of " + c["duplicate_of"]
        lines = []
        for event in c["context"]:
            captures = ", ".join(x["color"] + " " + x["piece"] for x in event["removed"]) or "—"
            lines.append(
                "<tr>"
                + "".join(
                    "<td>" + html.escape(str(x)) + "</td>"
                    for x in [event["ply"], event["color"], event["piece"], event["route"], captures]
                )
                + "</tr>"
            )
        rows.append(
            "<details><summary>"
            + html.escape(label)
            + "</summary><p>"
            + html.escape(detail)
            + "</p><table><tr><th>GUI ply</th><th>Side</th><th>Piece</th><th>Move (0-based engine squares)</th><th>Removed pieces</th></tr>"
            + "".join(lines)
            + "</table></details>"
        )
    page = '<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>V4 tactical candidates</title><style>body{font:15px system-ui;max-width:1100px;margin:24px auto;padding:0 16px;color:#202124;background:#fff}summary{cursor:pointer;padding:12px 0}details{border-bottom:1px solid #ccc}table{border-collapse:collapse;width:100%;font-size:13px}th,td{text-align:left;padding:6px;border-bottom:1px solid #ddd}h1{font-size:24px}p{line-height:1.5}</style><h1>V4 / v4.5 tactical candidates</h1>'
    page += f"<p>{len(corpus['games'])} loss games · {len(corpus['cases'])} episodes · {corpus['unique_cases']} unique decision cases. Captured {html.escape(corpus['captured'])}.</p>"
    page += (
        "<p>Scores use the losing agent’s perspective. These are candidates, not verified solutions. Open the original game in the GUI at the listed ply; expand a row for the observed continuation. Holdout cases are reserved for later evaluation.</p>"
        + "".join(rows)
        + "</html>"
    )
    (output / "review.html").write_text(page)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--inventory", type=Path, required=True)
    p.add_argument("--games", type=Path, required=True)
    p.add_argument("--replay", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--threshold", type=int, default=1200)
    a = p.parse_args()
    if a.threshold <= 0:
        p.error("threshold must be positive")
    c = build(a)
    print(
        json.dumps(
            dict(games=len(c["games"]), episodes=len(c["cases"]), unique=c["unique_cases"], holdout=c["holdout_slots"])
        )
    )
