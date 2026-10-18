"""Switch future analysis to the strongest teacher without relabelling old positions."""

import json
import sys

import artifact_storage

MIGRATION = "no-teacher-backfill-v1"


def retire_backfill(a, run, db, config):
    """Cancel legacy refreshes once, with the collector lock held and no live jobs.

    Old refreshes only selected already completed positions. Keep their labels,
    including archived teachers from interrupted refreshes, and restore completion.
    A missing valid saved label remains real work rather than a fabricated result.
    """
    db.execute("CREATE TABLE IF NOT EXISTS analysis_migrations(name TEXT PRIMARY KEY,payload TEXT)")
    saved = db.execute("SELECT payload FROM analysis_migrations WHERE name=?", (MIGRATION,)).fetchone()
    path = run / "analysis/dual-label-config.json"
    if saved:
        report = json.loads(saved["payload"])
    else:
        rows = db.execute(
            "SELECT id,payload FROM moments WHERE json_extract(payload,'$.teacher_refresh') IS NOT NULL "
            "AND id NOT IN (SELECT id FROM dual_paired)"
        ).fetchall()
        # One verified rollback for migration; subsequent teacher switches never
        # copy the growing database because they no longer change its rows.
        backup = artifact_storage.teacher_backup(db, path) if rows else None
        restored = []
        for row in rows:
            moment = json.loads(row["payload"])
            labels = [*moment.get("searches", []), *moment.get("previous_teacher_searches", [])]
            if any(
                moment.get("game_hash")
                and type(moment.get("center_ply")) is int
                and type(s.get("score")) is int
                and s.get("model_sha256")
                and s.get("ply") == moment.get("center_ply")
                and s.get("game_hash") == moment.get("game_hash")
                and (s.get("completed_depth", 0) > 0 or s.get("terminal"))
                for s in labels
            ):
                restored.append((row["id"],))
            else:
                print(f"backfill retirement: {row['id']} has no valid saved label; left unchanged", file=sys.stderr)
        report = dict(cancelled=len(restored), without_saved_label=len(rows) - len(restored))
        if backup:
            report["backup"] = str(backup)
        # Store the audit in the same transaction. If config publication fails,
        # restart recovers the original counts without modifying these rows again.
        with db:
            db.executemany("UPDATE moments SET status='completed',error=NULL WHERE id=?", restored)
            db.executemany("INSERT INTO dual_paired VALUES(?)", restored)
            db.executemany("DELETE FROM dual_failures WHERE id=?", restored)
            db.execute("INSERT INTO analysis_migrations VALUES(?,?)", (MIGRATION, json.dumps(report)))
        print(f"teacher backfill retired: {json.dumps(report)}", flush=True)
        if backup:
            artifact_storage.prune_teacher_backups(backup.parent)
    if "teacher_refresh" in config or config.get("backfill_retirement") != report:
        updated = dict(config, backfill_retirement=report)
        updated.pop("teacher_refresh", None)
        a.atomic(path, updated)
        config.clear()
        config.update(updated)
    return report


def replace(a, run, model, check_only=False, engine=None):
    control = run / "analysis"
    path = control / "dual-label-config.json"
    config = a.read(path)
    model = model.resolve()
    data = model.read_bytes()
    checkpoint = json.loads(data)
    name = checkpoint.get("name")
    if not name:
        raise ValueError("teacher checkpoint must have a name")
    a.validate_source(config["old"])
    helper = a.analyzer_for_agent(config["old"], {"engine": engine}) if engine else config
    a.subprocess.run([helper["analyzer_bin"], "--validate-model", str(model)], check=True)
    sha = a.digest(data)
    changed = config["new_sha256"] != sha or config.get("new_teacher_agent", {}).get("engine") != engine
    report = dict(teacher=name, sha256=sha, backfill_positions=0, changed=changed)
    if check_only or not changed:
        return report
    # Caller owns dual-label.lock. Snapshot before publishing; neither completed
    # positions nor pending work is invalidated by a change of default teacher.
    binding = a.snapshot_model(model, data, control)
    config.update(
        new_model=str(model),
        new_sha256=sha,
        new_binding=binding,
        new_teacher_id=name,
        new_teacher_agent=dict(name="ab", model=str(model), **({"engine": engine} if engine else {})),
    )
    config.pop("teacher_refresh", None)
    a.atomic(path, config)
    return report
