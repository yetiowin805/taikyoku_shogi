"""Deterministic, bounded sampling for training-label collection (no training)."""

import hashlib
import json

POLICY = "training-labels-v1"  # Search cache identity remains unchanged.
MATE_POLICY = "mate-recognition-v1"


def hashed(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def candidates(game):
    # Historical helpers remain the legacy analyzer's responsibility. A modern
    # frozen teacher must not silently run through a historical CLI binding.
    if any(game[side].get("engine") or game[side].get("name") != "ab" for side in ("black", "white")):
        return []
    moves = game["moves"]
    valid = [
        i
        for i, m in enumerate(moves)
        if type(m.get("eval")) is int and abs(m["eval"]) < 900_000 and m.get("color") in ("Black", "White")
    ]
    # Exclude agent/evaluation metadata from grouping related opening prefixes.
    prefix = [
        {
            k: v
            for k, v in m.items()
            if k in ("from_file", "from_rank", "to_file", "to_rank", "promoted", "data", "color")
        }
        for m in moves[:64]
    ]
    group = hashed([game.get("start"), prefix])
    split = "validation" if int(group[:8], 16) % 10 == 0 else "train"
    picked = {}

    def add(i, reason, pool_size, quota):
        if i not in picked:
            picked[i] = dict(
                center_ply=i + 1,
                plies=[i + 1],
                magnitude=0,
                selection=reason,
                selection_pool_size=pool_size,
                selection_quota=quota,
                split=split,
                split_group=group,
                sampling_policy=POLICY,
                game_result=game["result"],
                source_eval=moves[i]["eval"],
                score_perspective="black-absolute",
            )

    # Twelve equal-progress strata; pick by hash rather than always the middle.
    for bucket in range(12):
        pool = valid[len(valid) * bucket // 12 : len(valid) * (bucket + 1) // 12]
        if pool:
            i = min(pool, key=lambda i: hashed([group, "representative", i]))
            add(i, "representative", len(pool), 1)
    swings = []
    for i in valid:
        if i >= 2 and moves[i - 2].get("color") == moves[i]["color"]:
            old = moves[i - 2].get("eval")
            if type(old) is int and abs(old) < 900_000 and abs(moves[i]["eval"] - old) >= 1000:
                swings.append((abs(moves[i]["eval"] - old), i))
    # Space targeted examples too; avoid a single sequence filling the quota.
    selected = []
    for magnitude, i in sorted(swings, reverse=True):
        if i in picked or any(abs(i - j) < 4 for j in selected):
            continue
        add(i, "evaluation_swing", len(swings), 5)
        picked[i]["magnitude"] = magnitude
        selected.append(i)
        if len(selected) == 5:
            break
    # Keep ordinary selections, adding context without duplicate position rows.
    for i, contexts in mate_windows(game).items():
        add(i, "mate_recognition" if any(c["offset_plies"] == 0 for c in contexts) else "pre_mate_recognition", 10, 10)
        picked[i]["mate_recognitions"] = contexts
    return list(picked.values())


def mate_windows(game):
    """First recorded mate claim per player, never a declaration of proven mate."""
    moves = game["moves"]
    eligible = {i for i, m in enumerate(moves) if type(m.get("eval")) is int and m.get("color") in ("Black", "White")}
    picked = {}
    for side in ("Black", "White"):
        own = [i for i in sorted(eligible) if moves[i]["color"] == side]
        first = next((i for i in own if abs(moves[i]["eval"]) >= 900000), None)
        if first is None:
            continue
        score = moves[first]["eval"]
        sign = 1 if score > 0 else -1
        later = [i for i in own if i > first]
        retract = next((i for i in later if sign * moves[i]["eval"] < 900000), None)
        # Missing evaluations make persistence unknown, not affirmative evidence.
        missing = any(m.get("color") == side and type(m.get("eval")) is not int for m in moves[first + 1 :])
        persistence = "retracted" if retract is not None else "unknown" if missing or not later else "persisted"
        for offset in (-16, -8, -4, -2, 0):
            i = first + offset
            if i not in eligible:
                continue
            picked.setdefault(i, []).append(
                dict(
                    policy=MATE_POLICY,
                    agent_side=side,
                    recognition_ply=first + 1,
                    offset_plies=offset,
                    claim_score=score,
                    claim="winning" if (score > 0) == (side == "Black") else "losing",
                    persistence=persistence,
                    retraction_ply=None if retract is None else retract + 1,
                )
            )
    return picked


def scan_mates(a, db, run, state):
    """Incremental backfill for all modern games, including the winner sampler."""
    db.execute("CREATE TABLE IF NOT EXISTS mate_scanned(policy TEXT,path TEXT,PRIMARY KEY(policy,path))")
    db.execute("CREATE INDEX IF NOT EXISTS moments_game_hash ON moments(json_extract(payload, '$.game_hash'))")
    for slot in state["slots"]:
        if slot["status"] != "done" or not slot.get("game_path"):
            continue
        path = a.absolute(slot["game_path"])
        if db.execute("SELECT 1 FROM mate_scanned WHERE policy=? AND path=?", (MATE_POLICY, str(path))).fetchone():
            continue
        try:
            data = path.read_bytes()
            game = json.loads(data)
            if game.get("abort_reason") or game.get("result") is None:
                continue
            game_hash = a.digest(data)
            samples = [m for m in candidates(game) if m.get("mate_recognitions")]
            existing = (
                {
                    json.loads(r["payload"])["center_ply"]: r
                    for r in db.execute(
                        "SELECT * FROM moments WHERE json_extract(payload,'$.game_hash')=?", (game_hash,)
                    )
                }
                if samples
                else {}
            )
            for sample in samples:
                old = existing.get(sample["center_ply"])
                if old:
                    moment = json.loads(old["payload"])
                    moment["mate_recognitions"] = sample["mate_recognitions"]
                    db.execute("UPDATE moments SET payload=? WHERE id=?", (json.dumps(moment), old["id"]))
                    # Preserve completed labels and state, including dual_paired.
                    if old["status"] == "completed":
                        a.atomic(run / "analysis/moments" / (old["id"] + ".json"), moment)
                else:
                    key = a.digest(f"{POLICY}:{game_hash}:{sample['center_ply']}".encode())
                    sample.update(
                        id=key, game=str(path), game_hash=game_hash, game_id=game["game_id"], slot_id=slot["id"]
                    )
                    db.execute(
                        "INSERT INTO moments VALUES(?,?,'pending',?,NULL)",
                        (key, sample["magnitude"], json.dumps(sample)),
                    )
            db.execute("INSERT INTO mate_scanned VALUES(?,?)", (MATE_POLICY, str(path)))
            db.commit()
        except (OSError, ValueError, KeyError, TypeError) as exc:
            db.rollback()
            print(f"mate scan: retry {path}: {exc}", flush=True)
