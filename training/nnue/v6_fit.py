"""Four v6 objectives, shared quantization-aware training and resumable windows."""

import argparse
import gc
import json
import math
import random
import signal
import time
from pathlib import Path

import numpy as np
import torch
import torch.nn.functional as F

import v6_data as data
import v6_dataset as dataset
from pilot_fit import PilotNet, from_quantized
from plateau import Plateau, Policy
from quantized import Quantized
from train import atomic_json, batch, export

STOP = False


def stop(*_):
    global STOP
    STOP = True


def objective(logits, q, outcome, confidence, regime):
    if regime == "C":
        target = torch.logit(q.clamp(0.01, 0.99))
        return confidence * (
            0.8 * 0.25 * F.huber_loss(logits, target, reduction="none", delta=1)
            + 0.2 * 2 * (torch.sigmoid(logits) - q).square()
        )
    target = torch.where(torch.isfinite(outcome), 0.85 * q + 0.15 * torch.nan_to_num(outcome), q)
    return F.binary_cross_entropy_with_logits(logits, target, reduction="none")


def prediction(net, samples, features, ids, alpha, scale):
    x, offsets, _ = batch(samples, features, ids)
    material = torch.tensor([samples[i]["material"] for i in ids], dtype=torch.float32)
    return ((net(x, offsets) * 1000).clamp(-100000, 100000) + alpha * material) / scale


def tensors(samples, ids):
    return [
        torch.tensor([float("nan") if samples[i][key] is None else samples[i][key] for i in ids], dtype=torch.float32)
        for key in ("q", "outcome", "confidence")
    ]


def evaluate(net, samples, features, ids, alpha, scale):
    # Representative held-out groups only. Each group/game receives equal mass.
    counts = {}
    for i in ids:
        r = samples[i]
        counts.setdefault(r["group"], {}).setdefault(r["game"], 0)
        counts[r["group"]][r["game"]] += 1
    totals = dict(teacher=0.0, outcome=0.0, outcome_mass=0.0)
    with torch.no_grad():
        for i in ids:
            r = samples[i]
            w = 1 / len(counts) / len(counts[r["group"]]) / counts[r["group"]][r["game"]]
            p = torch.sigmoid(prediction(net, samples, features, [i], alpha, scale)).item()
            totals["teacher"] += w * (p - r["q"]) ** 2
            if r["outcome"] is not None:
                totals["outcome"] += w * (p - r["outcome"]) ** 2
                totals["outcome_mass"] += w
    if totals["outcome_mass"]:
        totals["outcome"] /= totals["outcome_mass"]
    totals["selection"] = (
        0.85 * totals["teacher"] + 0.15 * totals["outcome"] if totals["outcome_mass"] else totals["teacher"]
    )
    if not all(math.isfinite(v) for v in totals.values()):
        raise ValueError("nonfinite validation")
    return totals


def save_weights(net, path):
    temp = path.with_suffix(".tmp")
    torch.save(net.state_dict(), temp)
    temp.replace(path)


def load_net(path, features, width):
    weights = torch.load(path, weights_only=True, mmap=True, map_location="cpu")
    with torch.device("meta"):
        net = PilotNet(features, width)
    net.load_state_dict(weights, assign=True)
    return net


def publish(net, out, parent, schema, samples, features, ids, mode, scale):
    blob = out / "candidate.bin"
    sha = export(net, blob, schema, output_mode=mode)
    quant = Quantized(blob, schema["features"])
    if quant.output_mode != mode:
        raise ValueError("output mode changed during export")
    errors = []
    with torch.no_grad():
        for i in ids[:: max(1, len(ids) // 64)][:64]:
            r = samples[i]
            start = r["offset"]
            middle = start + r["us"]
            x, o, _ = batch(samples, features, [i])
            errors.append(
                abs(
                    quant.residual(features[start:middle], features[middle : middle + r["them"]])
                    - max(-100000, min(100000, net(x, o).item() * 1000))
                )
            )
    if max(errors) > 2:
        raise ValueError("quantized export parity failed: " + str(max(errors)))
    del quant
    target = out / ("nnue-" + sha + ".bin")
    blob.replace(target)
    cp = json.loads(Path(parent).read_text())
    cp["name"] = out.name
    # Keep the new-agent default when the pinned parent predates QFix defaults.
    if cp.setdefault("search_defaults", {}).get("experiment", "baseline") == "baseline":
        cp["search_defaults"]["experiment"] = "q_fix"
    cp["weights"]["nnue"] = dict(
        file=target.name, sha256=sha, width=net.width, feature_hash=schema["names_hash"], output_mode=mode
    )
    old = json.loads((out / "model.json").read_text()) if (out / "model.json").exists() else None
    atomic_json(out / "model.json", cp)
    if old and old["weights"]["nnue"]["file"] != target.name:
        (out / old["weights"]["nnue"]["file"]).unlink(missing_ok=True)
    return dict(max_score_error=max(errors), positions=len(errors), output_mode=mode)


def fit(config):
    global STOP
    STOP = False
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    torch.set_num_threads(config.get("threads", 1))
    torch.set_num_interop_threads(1)
    torch.manual_seed(config["seed"])
    out = Path(config["out"])
    out.mkdir(parents=True, exist_ok=True)
    source = Path(config["dataset"])
    identity = dataset.verify(source)
    samples = [json.loads(line) for line in (source / "samples.jsonl").read_text().splitlines()]
    features = np.memmap(source / "features.bin", dtype="<u4", mode="r")
    schema = json.loads((source / "schema.json").read_text())
    scale = identity["scale"]
    pairs = json.loads((source / "pairs.json").read_text())
    train_ids = [i for i, r in enumerate(samples) if r["split"] == "train"]
    val = [i for i, r in enumerate(samples) if r["split"] == "validation" and r["stratum"] == "representative"]
    if not train_ids or not val:
        raise ValueError("missing training or representative validation")
    conversion = config.get("prelude", False)
    regime = config["regime"]
    retain_material = config.get("retain_material", False)
    if not isinstance(retain_material, bool) or (retain_material and (regime not in "CD" or conversion)):
        raise ValueError("retain_material is only supported for main C/D runs")
    width = config["width"]
    if regime not in "ABCD" or len(regime) != 1:
        raise ValueError("unknown regime")
    if regime == "D" and not pairs:
        raise ValueError("ranking regime requires verified pairs")
    parent = json.loads(Path(config["parent"]).read_text())
    desc = parent["weights"]["nnue"]
    blob = Path(config["parent"]).parent / desc["file"]
    if data.file_hash(blob) != desc["sha256"] or desc["width"] != width or desc["feature_hash"] != schema["names_hash"]:
        raise ValueError("parent identity or schema mismatch")
    recipe = dict(
        config, dataset_sha256=data.file_hash(source / "dataset.json"), parent_sha256=data.file_hash(config["parent"])
    )
    if config.get("start"):
        recipe["start_sha256"] = data.file_hash(config["start"])
    recipe_path = out / "recipe.json"
    if recipe_path.exists() and json.loads(recipe_path.read_text()) != recipe:
        raise ValueError("resume recipe changed")
    atomic_json(recipe_path, recipe)
    if (out / "complete.json").exists():
        return
    checkpoint = out / "training.pt"
    start = 0
    best = math.inf
    metrics = []
    state = None
    resume_offset, resume_loss, sampler_state = 0, 0.0, None
    saved_conversion_baseline = None
    policy = Policy(min_epochs=4, max_epochs=None, patience=3, lr_reductions=2)
    tracker = Plateau(math.inf)
    if checkpoint.exists():
        state = torch.load(checkpoint, weights_only=True, mmap=True, map_location="cpu")
        if state["recipe"] != data.hashed(recipe):
            raise ValueError("checkpoint identity mismatch")
        with torch.device("meta"):
            net = PilotNet(schema["features"], width)
        net.load_state_dict(state["net"], assign=True)
        start = state["window"]
        best = state["best"]
        metrics = state["metrics"]
        tracker = Plateau(**state["tracker"])
        resume_offset = state.get("offset", 0)
        resume_loss = state.get("running_loss", 0.0)
        sampler_state = state.get("sampler")
        saved_conversion_baseline = state.get("conversion_baseline")
    elif config.get("start"):
        net = load_net(Path(config["start"]), schema["features"], width)
    else:
        net = from_quantized(blob, schema["features"])
    emb = torch.optim.SGD([net.embedding.weight], lr=1e-4)
    dense = torch.optim.Adam([v for k, v in net.named_parameters() if k != "embedding.weight"], lr=1.5e-4)
    if state:
        emb.load_state_dict(state["emb"])
        dense.load_state_dict(state["dense"])
        del state
        gc.collect()
    weights = [samples[i]["sampling_weight"] for i in train_ids]
    pair_weights = [p["weight"] for p in pairs]
    window_draws = config.get("window_draws", 65536)
    if window_draws < 8 or window_draws % 8:
        raise ValueError("window must contain complete batches of eight")
    # Conversion and matched A prelude both consume four windows. Only conversion
    # uses the training-only guard; validation never selects alpha-positive models.
    guard_ids = train_ids[:: max(1, len(train_ids) // 256)][:256]
    conversion_baseline = (
        saved_conversion_baseline
        if saved_conversion_baseline is not None
        else (
            metrics[0]["conversion_baseline"]
            if metrics
            else evaluate(net, samples, features, guard_ids, 1, scale)["teacher"]
        )
    )

    def persist(window, offset=0, running_loss=0.0, sampler=None):
        temp = checkpoint.with_suffix(".tmp")
        torch.save(
            dict(
                net=net.state_dict(),
                emb=emb.state_dict(),
                dense=dense.state_dict(),
                window=window,
                offset=offset,
                running_loss=running_loss,
                sampler=sampler,
                best=best,
                metrics=metrics,
                tracker=vars(tracker),
                recipe=data.hashed(recipe),
                conversion_baseline=conversion_baseline,
            ),
            temp,
        )
        temp.replace(checkpoint)

    window = start
    while not tracker.stopped and (not conversion or window < 4):
        if STOP:
            raise InterruptedError("training paused at a resumable window boundary")
        alpha = 1 if regime == "A" or retain_material else max(0, 1 - (window + 1) / 4) if conversion else 0
        rng = random.Random(config["seed"] + window)
        draws = rng.choices(train_ids, weights=weights, k=window_draws)
        running = resume_loss
        if sampler_state is not None:
            rng.setstate(sampler_state)
        started = time.time()
        for first in range(resume_offset, window_draws, 8):
            if STOP:
                persist(window, first, running, rng.getstate())
                raise InterruptedError("training paused at an effective-batch boundary")
            warmup = min(1.0, 0.1 + 0.9 * (window * window_draws + first + 8) / 32768)
            for optimizer, peak in ((emb, 1e-4), (dense, 1.5e-4)):
                for group in optimizer.param_groups:
                    group["lr"] = peak * warmup * (0.5**tracker.reductions)
            emb.zero_grad(set_to_none=True)
            dense.zero_grad(set_to_none=True)
            for i in draws[first : first + 8]:
                logits = prediction(net, samples, features, [i], alpha, scale)
                q, z, c = tensors(samples, [i])
                loss = objective(logits, q, z, c, regime) * (4 * (scale / 1000) ** 2) / 8
                if not torch.isfinite(loss).all():
                    raise ValueError("nonfinite training objective")
                loss.sum().backward()
                running += loss.item()
                net.embedding.weight.grad = net.embedding.weight.grad.coalesce()
            if regime == "D":
                pair = rng.choices(pairs, weights=pair_weights, k=1)[0]
                ids = [pair["a"], pair["b"]]
                u = prediction(net, samples, features, ids, alpha, scale)
                delta = u[0] * samples[ids[0]]["turn"] - u[1] * samples[ids[1]]["turn"]
                loss = (
                    0.10
                    * 4
                    * (scale / 1000) ** 2
                    * F.binary_cross_entropy_with_logits(delta, torch.tensor(pair["target"]))
                )
                if not torch.isfinite(loss):
                    raise ValueError("nonfinite ranking loss")
                loss.backward()
                net.embedding.weight.grad = net.embedding.weight.grad.coalesce()
            emb.step()
            dense.step()
            with torch.no_grad():
                touched = net.embedding.weight.grad.indices()[0]
                net.embedding.weight[touched] = net.embedding.weight[touched].clamp(-0.5, 0.5)
                net.bias.clamp_(-2, 2)
                for layer in (net.h1, net.h2):
                    layer.weight.clamp_(-127 / 64, 127 / 64)
                    layer.bias.clamp_(-8, 8)
            if first % 1024 == 0:
                atomic_json(
                    out / "status.json",
                    dict(state="training", window=window, draws=first, alpha=alpha, updated=time.time()),
                )
        # Validation/export can fail independently of optimization. A full-window
        # offset resumes directly at those checks without repeating the draws.
        persist(window, window_draws, running, rng.getstate())
        resume_offset, resume_loss, sampler_state = 0, 0.0, None
        validation = evaluate(net, samples, features, val, alpha, scale)
        guard = evaluate(net, samples, features, guard_ids, alpha, scale)["teacher"] if conversion else None
        if conversion and regime != "A" and guard > conversion_baseline + 0.05:
            raise ValueError("pure-NNUE conversion collapsed on training-only guard; no total model published")
        record = dict(
            window=window + 1,
            alpha=alpha,
            validation=validation,
            training_loss=running / (window_draws / 8),
            seconds=time.time() - started,
            conversion_baseline=conversion_baseline,
            conversion_guard=guard,
        )
        if not conversion and validation["selection"] < best:
            best = validation["selection"]
            record["export"] = publish(
                net,
                out,
                config["parent"],
                schema,
                samples,
                features,
                val,
                "residual" if regime == "A" or retain_material else "total",
                scale,
            )
        decision = tracker.observe(window + 1, validation["selection"], policy) if not conversion else "continue"
        if decision == "reduce_lr":
            for opt in (emb, dense):
                for group in opt.param_groups:
                    group["lr"] *= 0.5
        record["decision"] = decision
        metrics.append(record)
        window += 1
        persist(window)
        atomic_json(out / "metrics.json", metrics)
    if conversion:
        save_weights(net, out / "start.pt")
    atomic_json(
        out / "complete.json",
        dict(
            recipe=data.hashed(recipe),
            windows=window,
            draws=window * window_draws,
            reason="matched_prelude" if conversion else tracker.stopped,
            best=best if math.isfinite(best) else None,
        ),
    )
    atomic_json(out / "status.json", dict(state="completed", windows=window, updated=time.time()))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("config", type=Path)
    a = p.parse_args()
    try:
        fit(json.loads(a.config.read_text()))
    except InterruptedError:
        raise SystemExit(75) from None


if __name__ == "__main__":
    main()
