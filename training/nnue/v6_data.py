"""Frozen game selection and durable evidence catalogue for NNUE v6 collection."""

import hashlib
import json
import math
import shutil
import sqlite3
from collections import Counter, defaultdict
from pathlib import Path

POLICY = "nnue-v6-collection-v1"
MOVE_KEYS = ("color", "from_file", "from_rank", "to_file", "to_rank", "promoted", "data")


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def hashed(value):
    return hashlib.sha256(encoded(value).encode()).hexdigest()


def file_hash(path):
    result = hashlib.sha256()
    with Path(path).open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def probability(score, scale=2822.9057434005167):
    return 1 / (1 + math.exp(-max(-40, min(40, score / scale))))


def connect(out):
    db = sqlite3.connect(Path(out) / "catalogue.sqlite", timeout=30)
    db.row_factory = sqlite3.Row
    db.execute("PRAGMA journal_mode=WAL")
    db.execute("PRAGMA synchronous=FULL")
    db.executescript(
        "CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY,payload TEXT NOT NULL);"
        "CREATE TABLE IF NOT EXISTS games(id TEXT PRIMARY KEY,payload TEXT NOT NULL);"
        "CREATE TABLE IF NOT EXISTS parent_features(hash TEXT PRIMARY KEY);"
        "CREATE TABLE IF NOT EXISTS feature_exposure(hash TEXT NOT NULL,position_id TEXT PRIMARY KEY,split TEXT NOT NULL);"
        "CREATE INDEX IF NOT EXISTS feature_hashes ON feature_exposure(hash);"
        "CREATE TABLE IF NOT EXISTS positions(id TEXT PRIMARY KEY,game_id TEXT NOT NULL,ply INTEGER NOT NULL,"
        "payload TEXT NOT NULL,probe TEXT,holdout_eligible INTEGER NOT NULL DEFAULT 0);"
        "CREATE TABLE IF NOT EXISTS jobs(id TEXT PRIMARY KEY,position_id TEXT NOT NULL,teacher TEXT NOT NULL,"
        "tier TEXT NOT NULL,budget_ms INTEGER NOT NULL,priority INTEGER NOT NULL,status TEXT NOT NULL DEFAULT 'pending',"
        "attempts INTEGER NOT NULL DEFAULT 0,result TEXT,error TEXT);"
        "CREATE INDEX IF NOT EXISTS pending_jobs ON jobs(status,priority);"
        "CREATE TABLE IF NOT EXISTS iterations(job_id TEXT NOT NULL,depth INTEGER NOT NULL,payload TEXT NOT NULL,"
        "PRIMARY KEY(job_id,depth));"
    )
    return db


def get_meta(db, key):
    row = db.execute("SELECT payload FROM meta WHERE key=?", (key,)).fetchone()
    return json.loads(row[0]) if row else None


def set_meta(db, key, value):
    db.execute("INSERT OR REPLACE INTO meta VALUES(?,?)", (key, encoded(value)))


def parent_index(db, paths):
    # Shared storage knows how to read archived sample files and bound overlays.
    import dataset_storage

    games = defaultdict(set)
    identities = {}
    for path in paths:
        identities[str(path)] = dataset_storage.sample_digest(path)
        for row in dataset_storage.rows(path):
            game_hash = row.get("game_sha256", row.get("game"))
            if game_hash and row.get("group"):
                games[game_hash].add((row["group"], row["split"]))
            if row["split"] == "train":
                if not row.get("canonical_position_hash"):
                    raise ValueError(f"parent sample lacks canonical feature identity: {path}")
                db.execute("INSERT OR IGNORE INTO parent_features VALUES(?)", (row["canonical_position_hash"],))
    set_meta(db, "parent_samples", identities)
    return games


def assign_groups(records, prior, seed):
    roots = {key: key for key in records}

    def find(key):
        while roots[key] != key:
            roots[key] = roots[roots[key]]
            key = roots[key]
        return key

    seen = {}
    for key, row in sorted(records.items()):
        keys = [("pair", row["pair"]), ("prefix", row["prefix"])]
        if row["start_kind"] == "position":
            keys.append(("start", row["start_hash"]))
        keys.extend(("parent", group) for group, _ in prior.get(key, ()))
        for identity in keys:
            if identity in seen:
                roots[find(key)] = find(seen[identity])
            seen[identity] = key
    groups = defaultdict(list)
    for key in records:
        groups[find(key)].append(key)
    for members in groups.values():
        inherited = {item for key in members for item in prior.get(key, ())}
        splits = {split for _, split in inherited}
        group = min(members)
        bucket = int(hashed([seed, group])[:8], 16) % 10
        split = (
            next(iter(splits))
            if len(splits) == 1
            else "test"
            if bucket == 0
            else "validation"
            if bucket == 1
            else "train"
        )
        for key in members:
            records[key].update(group=group, split=split, quarantine=len(splits) > 1, inherited=bool(inherited))


def challenge_score(moves, index, result):
    move = moves[index]
    score = move.get("eval")
    if type(score) is not int:
        return 0.0, "unscored_structure"
    if abs(score) >= 900000:
        return 0.2, "mate_claim"
    value = probability(score)
    signals = [(0.0, "ordinary")]
    if type(move.get("static_eval")) is int:
        signals.append((abs(value - probability(move["static_eval"])), "saved_static_search_gap"))
    if index >= 2 and type(moves[index - 2].get("eval")) is int:
        signals.append((abs(value - probability(moves[index - 2]["eval"])), "saved_same_side_swing"))
    if result in ("BlackWins", "WhiteWins"):
        signals.append((0.5 * abs(value - (result == "BlackWins")), "confident_outcome_discrepancy"))
    if move.get("promoted"):
        signals.append((0.15, "promotion"))
    return max(signals)


def select_roots(game, game_hash, split, seed, representative=32, challenge=8, random_challenge_min=2):
    moves = game["moves"]
    valid = [i for i, move in enumerate(moves) if move.get("color") in ("Black", "White")]
    selected = {}
    for color in ("Black", "White"):
        pool = [i for i in valid if moves[i]["color"] == color and abs(moves[i].get("eval") or 0) < 900000]
        count = min(representative // 2, len(pool))
        for bucket in range(count):
            choices = pool[bucket * len(pool) // count : (bucket + 1) * len(pool) // count]
            choices.sort(key=lambda i: hashed([seed, game_hash, "representative", i]))
            chosen = next((i for i in choices if all(abs(i - j) >= 8 for j in selected)), None)
            if chosen is not None:
                selected[chosen] = dict(stratum="representative", selection="random_phase_color", priority_score=0)
    # No outcome/error mining in holdouts. Random challenges still cover all phases.
    pool = [i for i in valid if i not in selected]
    random_order = sorted(pool, key=lambda i: hashed([seed, game_hash, "challenge", i]))
    random_quota = max(random_challenge_min, math.ceil(challenge / 4)) if split == "train" else challenge
    targeted = sorted(pool, key=lambda i: (-challenge_score(moves, i, game["result"])[0], i))
    challenge_picks = []
    mate_claims = 0
    for random_pick, order, limit in ((True, random_order, random_quota), (False, targeted, challenge)):
        for i in order:
            if len(challenge_picks) >= limit:
                break
            if i in selected or any(abs(i - j) < 8 for j in challenge_picks):
                continue
            mate = abs(moves[i].get("eval") or 0) >= 900000
            if mate and mate_claims:
                continue
            score, reason = challenge_score(moves, i, game["result"])
            selected[i] = dict(
                stratum="challenge", selection="random_challenge" if random_pick else reason, priority_score=score
            )
            challenge_picks.append(i)
            mate_claims += mate
    return [
        dict(ply=i + 1, phase=min(3, 4 * i // max(1, len(moves))), color=moves[i]["color"], **meta)
        for i, meta in sorted(selected.items())
    ]


def add_position(db, payload):
    identity = hashed([POLICY, payload["game_id"], payload["ply"], payload.get("path", [])])
    payload = dict(payload, id=identity)
    db.execute(
        "INSERT OR IGNORE INTO positions(id,game_id,ply,payload) VALUES(?,?,?,?)",
        (identity, payload["game_id"], payload["ply"], encoded(payload)),
    )
    return identity


def add_job(db, position, teacher, tier, budget, priority):
    identity = hashed([POLICY, position, teacher, tier, budget])
    db.execute(
        "INSERT OR IGNORE INTO jobs(id,position_id,teacher,tier,budget_ms,priority) VALUES(?,?,?,?,?,?)",
        (identity, position, teacher, tier, budget, priority),
    )


def initialize(db, config):
    if get_meta(db, "initialized"):
        return
    out = Path(config["out"])
    state_path = out / "source-state.json"
    if not state_path.exists():
        raise ValueError("freeze source-state.json before preparing the collection")
    if file_hash(state_path) != config["state_sha256"]:
        raise ValueError("frozen source state changed")
    prior = parent_index(db, config["parent_samples"])
    state = json.loads(state_path.read_text())
    records = {}
    (out / "games").mkdir(exist_ok=True)
    for slot in state["slots"]:
        if slot["status"] != "done" or not slot.get("game_path"):
            continue
        source = Path(slot["game_path"])
        if not source.is_absolute():
            source = Path(config["source_repo"]) / source
        raw = source.read_bytes()
        game = json.loads(raw)
        if (
            game.get("abort_reason")
            or game.get("result") not in ("BlackWins", "WhiteWins", "Draw")
            or not game["moves"]
        ):
            continue
        digest = hashlib.sha256(raw).hexdigest()
        target = out / "games" / (digest + ".json")
        if not target.exists() and not config.get("sensitivity"):
            temp = target.with_suffix(".tmp")
            temp.write_bytes(raw)
            temp.replace(target)
        elif target.exists() and file_hash(target) != digest:
            raise ValueError("frozen game changed")
        prefix = [{key: move[key] for key in MOVE_KEYS if key in move} for move in game["moves"][:64]]
        records[digest] = dict(
            path=str(target),
            original=str(source),
            id=digest,
            slot=slot["id"],
            pair=hashed([state["run_id"], slot["start_seed"]]),
            prefix=hashed([game["start"], prefix]),
            start_kind=game["start"]["kind"],
            start_hash=hashed(game["start"]),
            moves=len(game["moves"]),
            result=game["result"],
            timestamp=game.get("timestamp"),
            outcome_eligible=False,
            outcome_reason="awaiting_terminal_or_clock_audit",
        )
    if not records:
        raise ValueError("no completed games in frozen source")
    assign_groups(records, prior, config["seed"])
    if config.get("sensitivity"):
        import v7_pilot_data

        records = v7_pilot_data.select_games(records, config)
        for row in records.values():
            target = Path(row["path"])
            if not target.exists():
                shutil.copyfile(row["original"], target)
            if file_hash(target) != row["id"]:
                raise ValueError("pilot source game changed during freezing")
    roots = []
    for digest, row in sorted(records.items()):
        db.execute("INSERT OR REPLACE INTO games VALUES(?,?)", (digest, encoded(row)))
        if row["quarantine"]:
            continue
        game = json.loads(Path(row["path"]).read_text())
        for selected in select_roots(
            game,
            digest,
            row["split"],
            config["seed"],
            config["representative"],
            config["challenge"],
            random_challenge_min=1 if config.get("sensitivity") else 2,
        ):
            payload = dict(
                selected,
                game_id=digest,
                group=row["group"],
                split=row["split"],
                inherited=row["inherited"],
                path=[],
                parent=None,
                saved=game["moves"][selected["ply"] - 1],
                outcome=game["result"],
                outcome_eligible=False,
            )
            roots.append((add_position(db, payload), payload))
    # Pilot is representative training data, balanced across games before taking a second root.
    pilot_pool = sorted(
        ((key, row) for key, row in roots if row["split"] == "train" and row["stratum"] == "representative"),
        key=lambda item: hashed([config["seed"], "pilot", item[0]]),
    )
    pilot = []
    used = set()
    for key, row in pilot_pool:
        if row["game_id"] not in used:
            pilot.append(key)
            used.add(row["game_id"])
        if len(pilot) == config["pilot_roots"]:
            break
    pilot_set = set(pilot)
    for key, _ in roots:
        add_job(db, key, config["primary"], "broad", config["budgets"]["broad"], 0 if key in pilot_set else 10)
    for digest, row in records.items():
        if row["quarantine"]:
            continue
        position = add_position(
            db,
            dict(
                game_id=digest,
                ply=row["moves"] + 1,
                path=[],
                parent=None,
                group=row["group"],
                split=row["split"],
                inherited=row["inherited"],
                stratum="outcome_audit",
                outcome=row["result"],
                outcome_eligible=False,
            ),
        )
        add_job(db, position, config["primary"], "outcome", 0, 15)
    for key in pilot[: config["pilot_challenger"]]:
        add_job(db, key, config["challenger"], "pilot", config["budgets"]["broad"], 1)
        if config.get("diagnostic"):
            add_job(db, key, config["diagnostic"], "pilot", config["budgets"]["broad"], 1)
    set_meta(db, "pilot", pilot)
    set_meta(
        db,
        "initialized",
        dict(games=len(records), roots=len(roots), splits=dict(Counter(r["split"] for r in records.values()))),
    )
    db.commit()


def root_rows(db, config, tier="broad"):
    return [
        dict(json.loads(row["payload"]), result=json.loads(row["result"]))
        for row in db.execute(
            "SELECT p.payload,j.result FROM positions p JOIN jobs j ON j.position_id=p.id "
            "WHERE j.teacher=? AND j.tier=? AND j.status='completed'",
            (config["primary"], tier),
        )
    ]


def mixed_selection(rows, count, salt, score):
    """Reserve half random, then target training-only errors; cap repeated games."""
    selected = []
    seen = set()
    mass = Counter()
    random_order = sorted(rows, key=lambda row: hashed([salt, row["id"]]))
    target_order = sorted(
        rows, key=lambda row: (-score(row) if row["split"] == "train" else 0, hashed([salt, row["id"]]))
    )
    for order, limit in ((random_order, (count + 1) // 2), (target_order, count)):
        for row in order:
            if len(selected) >= limit:
                break
            if row["id"] in seen or mass[row["game_id"]] >= 8:
                continue
            selected.append(row)
            seen.add(row["id"])
            mass[row["game_id"]] += 1
    return selected


def evidence_priority(row):
    evidence = row["result"]
    score = evidence.get("label", {}).get("score_black")
    probe = evidence.get("probe", {})
    if score is None:
        return 0
    value = probability(score)
    q = probe.get("quiescence", {})
    return max(
        abs(value - probability(probe.get("static_black", score))),
        abs(value - probability(q["score_black"])) if not q.get("aborted", True) else 0,
    )


def plan_audits(db, config):
    if get_meta(db, "audit_planned"):
        return
    rows = [row for row in root_rows(db, config) if row["result"].get("label")]
    if config.get("full_v7"):
        from v7_full_data import audit_selection

        audit = audit_selection(rows, 0.10, [config["seed"], "audit"], evidence_priority)
    else:
        audit = mixed_selection(rows, config["audit_roots"], [config["seed"], "audit"], evidence_priority)
    for row in audit:
        for teacher in (config["primary"], config["challenger"]):
            add_job(db, row["id"], teacher, "audit", config["budgets"]["audit"], 20)
    if config.get("sensitivity"):
        import v7_pilot_data

        v7_pilot_data.plan(db, config, rows)
        set_meta(db, "audit_planned", dict(roots=len(audit), sensitivity=True))
        db.commit()
        return
    training = [row for row in rows if row["split"] == "train"]
    branches = mixed_selection(training, config["branch_roots"], [config["seed"], "branches"], evidence_priority)
    for row in branches:
        add_job(db, row["id"], config["primary"], "alternatives", 0, 30)
    set_meta(db, "audit_planned", dict(roots=len(audit), branch_roots=len(branches)))
    db.commit()


def plan_deep(db, config):
    if get_meta(db, "deep_planned"):
        return
    rows = [row for row in root_rows(db, config, "audit") if row["result"].get("label")]
    for row in rows:
        other = db.execute(
            "SELECT result FROM jobs WHERE position_id=? AND teacher=? AND tier='audit' AND status='completed'",
            (row["id"], config["challenger"]),
        ).fetchone()
        row["gap"] = 0
        if other and (label := json.loads(other[0]).get("label")):
            row["gap"] = abs(probability(row["result"]["label"]["score_black"]) - probability(label["score_black"]))
    if config.get("full_v7"):
        from v7_full_data import audit_selection

        deep = audit_selection(
            rows, 0.20, [config["seed"], "deep"], lambda row: max(row["gap"], evidence_priority(row))
        )
    else:
        deep = mixed_selection(
            rows, config["deep_roots"], [config["seed"], "deep"], lambda row: max(row["gap"], evidence_priority(row))
        )
    for row in deep:
        for teacher in (config["primary"], config["challenger"]):
            add_job(db, row["id"], teacher, "deep", config["budgets"]["deep"], 40)
    set_meta(db, "deep_planned", dict(roots=len(deep)))
    db.commit()


def lease_indices(config):
    return config.get("lease_indices", [config["lease_index"]])


def validate_config(config):
    if config.get("policy") != POLICY or not 1 <= config.get("workers", 0) <= 6:
        raise ValueError("v6 collection requires policy v1 and one to six workers")
    indices = lease_indices(config)
    if (
        len(indices) != config["workers"]
        or len(set(indices)) != len(indices)
        or any(i in (0, 2) or i not in range(len(config["cpus"])) for i in indices)
        or len({config["cpus"][i] for i in indices}) != len(indices)
    ):
        raise ValueError("invalid, duplicate or reserved tournament leases")
    for name in ("audit_roots", "deep_roots", "branch_roots", "branch_audit_roots", "branch_deep_roots"):
        if config[name] < 0:
            raise ValueError("negative collection quota")
    if config["deep_roots"] > config["audit_roots"] or not (
        config["branch_deep_roots"] <= config["branch_audit_roots"] <= config["branch_roots"]
    ):
        raise ValueError("verification quotas exceed their source pools")
    if config["primary"] == config["challenger"] or not config["parent_samples"]:
        raise ValueError("two teachers and parent exposure manifests are required")
    if config["representative"] <= 0 or config["representative"] % 2 or config["challenge"] < 2:
        raise ValueError("invalid per-game quotas")
    if not 0 < config["budgets"]["broad"] < config["budgets"]["audit"] < config["budgets"]["deep"]:
        raise ValueError("search budgets must be positive and increasing")
    if config["pilot_roots"] < 1 or config["max_attempts"] < 1:
        raise ValueError("positive pilot and retry limits required")
    for path, expected in config["identities"].items():
        if file_hash(path) != expected:
            raise ValueError("frozen input changed: " + path)
    if shutil.disk_usage(config["out"]).free < config["minimum_free_bytes"]:
        raise ValueError("insufficient free storage for collection")
