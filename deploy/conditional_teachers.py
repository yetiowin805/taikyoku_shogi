"""Fixed primary teacher with outcome-conditioned, position-local secondaries."""

import json
from pathlib import Path

POLICY = "conditional-512v4-v1"


def select(game, ply):
    """Return a secondary and auditable evidence; recorded scores are black-absolute."""
    moves = game["moves"]
    i = ply - 1
    evidence = {"policy": POLICY, "reason": "missing_adjacent_evaluations"}
    if not 0 < i < len(moves) - 1:
        return None, evidence
    before, current, after = moves[i - 1 : i + 2]
    color = current.get("color")
    other = {"Black": "White", "White": "Black"}.get(color)
    if other is None or before.get("color") != other or after.get("color") != other:
        return None, evidence
    if any(type(m.get("eval")) is not int for m in (before, current, after)):
        return None, evidence
    scores = {color: current["eval"], other: (before["eval"] + after["eval"]) / 2}
    evidence.update(black_estimate=scores["Black"], white_estimate=scores["White"], reason="no_rule_matched")
    winner = {"BlackWins": "Black", "WhiteWins": "White"}.get(game.get("result"))
    selected = None
    if winner:
        sign = 1 if winner == "Black" else -1
        loser = "White" if winner == "Black" else "Black"
        if scores["Black"] > 0 > scores["White"]:
            selected, evidence["reason"] = winner, "mutual_optimism"
        elif (
            sign * scores[winner] > 0
            and sign * scores[loser] > 0
            and sign * scores[winner] >= 1.2 * sign * scores[loser]
        ):
            selected, evidence["reason"] = winner, "winner_advantage_20_percent"
    elif game.get("result") == "Draw" and scores["Black"] * scores["White"] > 0:
        # Equal magnitudes tie deterministically in favor of Black.
        selected = min(("Black", "White"), key=lambda side: abs(scores[side]))
        evidence["reason"] = "draw_more_conservative"
    evidence["secondary_side"] = selected
    if selected is None:
        return None, evidence
    agent = game[selected.lower()]
    if agent.get("name") != "ab":
        raise ValueError(f"unsupported secondary teacher: {agent}")
    return {k: agent[k] for k in ("name", "model", "engine") if k in agent}, evidence


def searches(a, dual, config, moment, db):
    if config.get("new_teacher_id") != "NNUE_W512_v4":
        raise ValueError("conditional policy requires frozen NNUE_W512_v4 primary")
    data = Path(moment["game"]).read_bytes()
    if a.digest(data) != moment["game_hash"]:
        raise ValueError("game changed since scan")
    agent, evidence = select(json.loads(data), moment["center_ply"])
    primary = dual.teacher_config(config, True)
    teachers = [primary]
    if agent:
        teachers.append(
            dict(
                primary,
                label_teacher=agent,
                label_teacher_id="conditional-secondary",
                rolling_engines=False if agent.get("engine") else primary.get("rolling_engines", False),
            )
        )
    old = [*moment.get("previous_teacher_searches", []), *moment.get("searches", [])]
    archived = {s.get("key", s.get("model_sha256")): s for s in old}
    moment["previous_teacher_searches"] = list(archived.values())
    moment["teacher_selection"] = evidence
    moment["searches"] = []
    seen = set()
    for teacher in teachers:
        spec = teacher["label_teacher"]
        key = str(a.absolute(spec["model"]).resolve())
        binding = teacher["models"][key]
        identity = (binding["sha256"], spec.get("engine"))
        if identity in seen:
            continue
        seen.add(identity)
        # Existing completed labels remain valid evidence across rolling updates;
        # retain their engine provenance rather than refreshing solely for age.
        retained = [
            s
            for s in old
            if s.get("model_sha256") == binding["sha256"]
            and s.get("budget_ms") == config["budget_ms"]
            and s.get("game_hash") == moment["game_hash"]
            and s.get("ply") == moment["center_ply"]
            and s.get("agent", {}).get("engine") == spec.get("engine")
            and type(s.get("score")) is int
            and s.get("completed_depth", 0) > 0
        ]
        result = (
            max(retained, key=lambda s: s["completed_depth"])
            if retained
            else a.search_position(teacher, moment, moment["center_ply"], db)
        )
        if result.get("model_sha256") != binding["sha256"]:
            raise ValueError("conditional teacher model identity mismatch")
        moment["searches"].append(result)
        db.execute("UPDATE moments SET payload=? WHERE id=?", (json.dumps(moment), moment["id"]))
        db.commit()
    return moment


def requeue(db, config):
    """Once per policy, reconsider existing rows; immutable search cache is retained."""
    if config.get("teacher_policy") != POLICY:
        return
    db.execute("CREATE TABLE IF NOT EXISTS teacher_policy_applied(policy TEXT PRIMARY KEY)")
    if db.execute("SELECT 1 FROM teacher_policy_applied WHERE policy=?", (POLICY,)).fetchone():
        return
    with db:
        db.execute("UPDATE moments SET status='pending',error=NULL")
        db.execute("DELETE FROM dual_paired")
        db.execute("DELETE FROM dual_failures")
        db.execute("INSERT INTO teacher_policy_applied VALUES(?)", (POLICY,))


def enable(a, refresh, run, model):
    if a.read(model).get("name") != "NNUE_W512_v4":
        raise ValueError("--model must be NNUE_W512_v4")
    # Caller holds the collector lock. Replacement validates and snapshots the model.
    refresh.replace(a, run, model)
    path = run / "analysis/dual-label-config.json"
    config = a.read(path)
    db = a.connect(run, "training-labels.sqlite")
    try:
        refresh.artifact_storage.teacher_backup(db, path)
    finally:
        db.close()
    config.update(teacher_policy=POLICY, new_teacher_id="NNUE_W512_v4")
    config.pop("auto_champion", None)
    config.pop("teacher_refresh", None)
    a.atomic(path, config)
    db = a.connect(run, "training-labels.sqlite")
    try:
        requeue(db, config)
    finally:
        db.close()
    return {"policy": POLICY, "primary": "NNUE_W512_v4", "existing_positions": "requeued; cached searches retained"}
