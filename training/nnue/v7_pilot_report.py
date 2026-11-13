"""Matched-window predictions, group uncertainty and isolated move-choice screening."""

import json
import os
import random
import statistics
import sys
import time
from collections import defaultdict
from pathlib import Path

import v6_data as data
from v6_collect import BoundaryLeases, Oracle, accepted_label, allocation_busy, atomic
from v6_runtime import LeasePool
from v7_pilot import VARIANTS, tasks


def predictions(net, config, window, scale):
    import numpy as np
    import torch

    from v6_fit import prediction

    source = Path(config["pilot_probe_dataset"])
    features = np.memmap(source / "features.bin", dtype="<u4", mode="r")
    rows = [json.loads(line) for line in (source / "samples.jsonl").read_text().splitlines()]
    result = []
    with torch.no_grad():
        for i, row in enumerate(rows):
            if row["split"] != "validation":
                continue
            p = torch.sigmoid(prediction(net, rows, features, [i], 1, scale)).item()
            result.append(
                dict(
                    id=row["id"],
                    group=row["group"],
                    q=row["q"],
                    outcome=row["outcome"],
                    p=p,
                    views=row["pilot_views"],
                    phase=row["phase"],
                    promotion=row["legal_promotion"],
                    range_two_movers=row["range_two_movers"],
                )
            )
    atomic(Path(config["out"]) / f"predictions-{window}.json", result)


def interval(values):
    if len(values) < 2:
        return None
    rng = random.Random(20261112)
    means = sorted(statistics.mean(rng.choices(values, k=len(values))) for _ in range(500))
    return [means[12], means[487]]


def contrast(base, candidate):
    if {r["id"] for r in base} != {r["id"] for r in candidate}:
        raise ValueError("comparison probe positions differ")
    other = {r["id"]: r for r in candidate}
    groups = defaultdict(list)
    for row in base:
        if "S1:representative" in row["views"]:
            groups[row["group"]].append((row, other[row["id"]]))
    shifts, teacher, outcome = [], [], []
    for pairs in groups.values():
        shifts.append(statistics.mean(abs(a["p"] - b["p"]) for a, b in pairs))
        teacher.append(statistics.mean((b["p"] - a["q"]) ** 2 - (a["p"] - a["q"]) ** 2 for a, b in pairs))
        eligible = [(a, b) for a, b in pairs if a["outcome"] is not None]
        if eligible:
            outcome.append(
                statistics.mean((b["p"] - a["outcome"]) ** 2 - (a["p"] - a["outcome"]) ** 2 for a, b in eligible)
            )
    changes = sorted(abs(a["p"] - other[a["id"]]["p"]) for a in base)
    if not shifts or not changes:
        return dict(classification="inconclusive: no common representative groups")
    mean = statistics.mean(shifts)
    p90 = changes[int(0.9 * (len(changes) - 1))]
    slices = {}
    for key, condition in (
        ("promotion", lambda r: r["promotion"]),
        ("range_two_movers", lambda r: bool(r["range_two_movers"])),
        ("S3 alternatives", lambda r: "S3:alternative" in r["views"]),
        ("S2 settled", lambda r: any(v.startswith("S2:settled") for v in r["views"])),
    ):
        selected = [r for r in base if condition(r)]
        grouped = defaultdict(list)
        for r in selected:
            grouped[r["group"]].append(abs(r["p"] - other[r["id"]]["p"]))
        if grouped:
            slices[key] = dict(
                groups=len(grouped), mean_abs_probability=statistics.mean(statistics.mean(v) for v in grouped.values())
            )
    active = (
        mean >= 0.005
        or p90 >= 0.02
        or any(v["groups"] >= 20 and v["mean_abs_probability"] >= 0.005 for v in slices.values())
    )
    return dict(
        groups=len(groups),
        mean_abs_probability=mean,
        row_p90_abs_probability=p90,
        teacher_brier_delta=statistics.mean(teacher),
        teacher_group_bootstrap_95=interval(teacher),
        outcome_brier_delta=statistics.mean(outcome) if outcome else None,
        outcome_group_bootstrap_95=interval(outcome),
        outcome_groups=len(outcome),
        slices=slices,
        classification="inconclusive: fewer than 20 independent validation groups"
        if len(groups) < 20
        else "material prediction effect; direction requires review"
        if active
        else "small effect at tested scale",
    )


def move_panel(root, config, deadline):
    """One leased CPU; fixed common roots, candidate moves, then a common teacher."""
    training = root / "training"
    rows = [json.loads(line) for line in (training / "dataset/samples.jsonl").read_text().splitlines()]
    by_group = {}
    for row in sorted(rows, key=lambda r: data.hashed(r["id"])):
        if row["split"] == "validation" and "S1:representative" in row["pilot_views"]:
            by_group.setdefault(row["group"], row)
    selected = list(by_group.values())[:16]
    models = {
        job["name"]: str(training / job["name"] / "model.json")
        for job in tasks(config)
        if not job["fit"].get("prelude")
        and job["fit"]["seed"] == 20261112
        and (training / job["name"] / "complete.json").exists()
    }
    result = []
    teacher = teachers_snapshot(root)
    output = root / "move-panel"
    output.mkdir(exist_ok=True)

    def search(row, model, path, milliseconds):
        oracle = Oracle(
            dict(binary=config["binary"], request_timeout_seconds=90),
            dict(game=row["game_path"], start_ply=1),
            output,
            model,
        )
        events = []
        try:
            final = oracle.call(
                dict(op="search", root_ply=row["ply"], path=path, depth=64, time_ms=milliseconds),
                lambda e: events.append(e) if e["event"] == "iteration" else None,
            )
            return accepted_label(events, final)
        finally:
            oracle.close()

    for row in selected:
        if time.time() >= deadline - 120 or allocation_busy(Path(config["control"])):
            break
        choices = {name: search(row, path, row["path"], 2000) for name, path in models.items()}
        distinct = {
            data.encoded(label["best"]): label["best"] for label in choices.values() if label and label.get("best")
        }
        adjudication = {}
        if len(distinct) > 1:
            for key, move in distinct.items():
                if time.time() >= deadline - 60:
                    break
                adjudication[key] = search(row, teacher, row["path"] + [move], 10000)
        scored = {key: label["score_black"] * row["turn"] for key, label in adjudication.items() if label}
        regrets = {
            name: max(scored.values()) - scored[data.encoded(label["best"])]
            for name, label in choices.items()
            if label and data.encoded(label["best"]) in scored
        }
        result.append(
            dict(
                id=row["id"],
                group=row["group"],
                choices=choices,
                teacher_children=adjudication,
                regret_in_teacher_score_units=regrets,
            )
        )
        atomic(
            root / "move-panel.json",
            dict(
                teacher=teacher,
                completed_roots=len(result),
                rows=result,
                interpretation="2-second candidate choices; 10-second teacher child scores in Black orientation; screening only",
            ),
        )
    return dict(
        roots=len(result),
        models=len(models),
        changed_choice_roots=sum(
            len({data.encoded(v["best"]) for v in r["choices"].values() if v}) > 1 for r in result
        ),
    )


def teachers_snapshot(root):
    import v6_teachers

    config = json.loads((root / "config.json").read_text())
    snapshot = v6_teachers.snapshot(config)
    atomic(root / "diagnostic-teacher.json", snapshot)
    return snapshot["models"]["primary"]["path"]


def report(root):
    config = json.loads((root / "pipeline.json").read_text())
    times = json.loads((root / "timing.json").read_text())
    out = root / "training"
    pool = LeasePool(Path(config["control"]), BoundaryLeases, reserve=1)
    lease = None
    try:
        while time.time() < times["deadline"]:
            acquired = pool.update(config["lease_indices"], allocation_busy(Path(config["control"])))
            if acquired:
                index, lease = acquired
                os.sched_setaffinity(0, {json.loads((root / "config.json").read_text())["cpus"][index]})
                os.environ["TACTICAL_LEASE_FD"] = str(lease.fileno())
                break
            time.sleep(1)
        comparisons = {}
        for regime, view in VARIANTS:
            for seed in (20261112, 20261113):
                name = f"pilot-256-{regime}-{view}-{seed}"
                control = f"pilot-256-{'C' if regime == 'Cu' else 'A'}-S1-{seed}"
                for window in (4, 8, 10, 12):
                    paths = [out / n / f"predictions-{window}.json" for n in (control, name)]
                    if all(p.exists() for p in paths):
                        comparisons[f"{name}/window-{window}"] = contrast(*(json.loads(p.read_text()) for p in paths))
        noise = {}
        for regime in ("A", "C"):
            for window in (4, 8, 10, 12):
                paths = [
                    out / f"pilot-256-{regime}-S1-{seed}/predictions-{window}.json" for seed in (20261112, 20261113)
                ]
                if all(p.exists() for p in paths):
                    noise[f"{regime}/window-{window}"] = contrast(*(json.loads(p.read_text()) for p in paths))
        actuation = {}
        reference_ids = set()
        for view in ("S1", "S2", "S3"):
            path = out / "views" / view / "samples.jsonl"
            if not path.exists():
                continue
            rows = [json.loads(line) for line in path.read_text().splitlines()]
            rows = [r for r in rows if r["split"] == "train"]
            if view == "S1":
                reference_ids = {r["id"] for r in rows}
            confidence = [r["confidence"] for r in rows]
            actuation[view] = dict(
                rows=len(rows),
                groups=len({r["group"] for r in rows}),
                unique_positions=len({r["id"] for r in rows}),
                mass_outside_S1=sum(r["sampling_weight"] for r in rows if r["id"] not in reference_ids),
                confidence_cv=statistics.pstdev(confidence) / statistics.mean(confidence),
                Ao_target_shift=sum(
                    r["sampling_weight"] * 0.15 * abs(r["outcome"] - r["q"]) for r in rows if r["outcome"] is not None
                ),
            )
        adaptive = {}
        for path in out.glob("pilot-256-E-*/metrics.json"):
            adaptive[path.parent.name] = [r["adaptive"] for r in json.loads(path.read_text()) if "adaptive" in r]
        panel = (
            move_panel(root, config, times["deadline"])
            if lease and (out / "dataset/dataset.json").exists()
            else dict(skipped="no leased capacity before deadline")
        )
        jobs = tasks(config)[1:]
        missing = [j["name"] for j in jobs if not (out / j["name"] / "complete.json").exists()]
        result = dict(
            comparisons=comparisons,
            seed_noise=noise,
            actuation=actuation,
            adaptive=adaptive,
            move_panel=panel,
            incomplete_runs=missing,
            interpretation="development sensitivity only; no Elo claim or final-test use",
            **times,
        )
        atomic(root / "report.json", result)
        lines = [
            "# V7 sensitivity pilot",
            "",
            f"Completed runs: {len(jobs) - len(missing)}/{len(jobs)}. Fixed material; legacy input schema.",
            "",
            "Matched-window changes use the same validation roots. Negative Brier deltas favor the variant. Bootstrap intervals resample related-game groups; compare both seeds with seed_noise in report.json.",
            "",
        ]
        for name, value in comparisons.items():
            if name.endswith("window-" + str(tasks(config)[-1]["fit"]["pilot_windows"])):
                lines.append(
                    f"- {name}: {value['classification']}; mean probability shift {value.get('mean_abs_probability', 0):.5f}, teacher Brier delta {value.get('teacher_brier_delta', 0):+.6f}."
                )
        lines.extend(
            [
                "",
                f"Incomplete runs: {', '.join(missing) or 'none'}.",
                "",
                "Inspect actuation and adaptive sections before interpreting a small effect. Missing windows, too few supported groups, or no adaptive updates make the relevant hypothesis inconclusive. The move panel is a shallow diagnostic, not a tournament result. Pilot validation is now development data.",
            ]
        )
        (root / "report.md").write_text("\n".join(lines) + "\n")
    finally:
        pool.close()
        if lease:
            lease.close()


if __name__ == "__main__":
    report(Path(sys.argv[1]))
