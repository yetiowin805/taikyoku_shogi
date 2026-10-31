"""CPU benchmark workload; calls the selected trainer without copying its loop."""

import cProfile
import hashlib
import inspect
import json
import os
import random
import resource
import sys
import tempfile
import time
from pathlib import Path

import dataset_storage


def clamp_parameters(net, touched):
    import torch

    with torch.no_grad():
        net.embedding.weight[touched] = net.embedding.weight[touched].clamp(-0.5, 0.5)
        net.bias.clamp_(-2, 2)
        for layer in (net.h1, net.h2):
            layer.weight.clamp_(-127 / 64, 127 / 64)
            layer.bias.clamp_(-8, 8)


def callable_profile(profiler, targets):
    entries = profiler.getstats()
    stages, missing = {}, []
    for name, function in targets.items():
        if function is None:
            selected = [
                entry
                for entry in entries
                if isinstance(entry.code, str) and entry.code.startswith("<method 'coalesce'")
            ]
        else:
            code = inspect.unwrap(function).__code__
            selected = [entry for entry in entries if entry.code is code]
        if selected:
            stages[name] = dict(
                calls=sum(e.callcount for e in selected),
                cumulative_s=sum(e.totaltime for e in selected),
                self_s=sum(e.inlinetime for e in selected),
            )
        else:
            missing.append(name)
    return dict(
        callables=stages,
        unavailable=missing,
        interpretation="Instrumented cumulative callable wall times can overlap; do not sum or compare with unprofiled timings.",
    )


def consume(a):
    setup_started = time.perf_counter()
    from archive_storage import digest

    os.sched_setaffinity(0, {a.cpu})
    sys.path.insert(0, str(a.module_dir))
    import numpy as np
    import torch

    import pilot_fit as fit

    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    torch.manual_seed(a.seed)
    manifest = (a.dataset / "dataset.json").read_bytes()
    identity = json.loads(manifest)
    for name, suffix in (("samples", "jsonl"), ("schema", "json")):
        actual = (
            dataset_storage.sample_digest(a.dataset / "samples.jsonl")
            if name == "samples"
            else digest(a.dataset / f"{name}.{suffix}")
        )
        if actual != identity[name + "_sha256"]:
            raise ValueError(f"Dataset {name} hash mismatch")
    samples = list(dataset_storage.rows(a.dataset / "samples.jsonl"))
    if "feature_shards" in identity:
        from v4_data import Features

        data = Features(a.dataset)
    else:
        feature_path = dataset_storage.hydrate(a.dataset / "features.bin")
        if digest(feature_path) != identity["features_sha256"]:
            raise ValueError("Dataset features hash mismatch")
        data = np.memmap(feature_path, mode="r", dtype="<u4")
    schema = json.loads((a.dataset / "schema.json").read_text())
    train = [i for i, sample in enumerate(samples) if sample["split"] == "train"]
    val = [i for i, sample in enumerate(samples) if sample["split"] == "validation"]
    if not train or not val:
        raise ValueError("Benchmark needs nonempty training and validation splits")
    weights = fit.weights_for(samples, train)
    rng = random.Random(a.seed)
    order = rng.choices(train, weights=[weights[i] for i in train], k=(a.steps + 1) * a.batch)
    validation = rng.sample(val, min(a.validation_samples, len(val)))
    net = fit.PilotNet(schema["features"], a.width)
    emb = torch.optim.SGD([net.embedding.weight], lr=0.0002)
    dense = torch.optim.Adam([v for k, v in net.named_parameters() if k != "embedding.weight"], lr=0.0003)
    scale = fit.calibration(samples)["scale"]

    def step(ids):
        emb.zero_grad(set_to_none=True)
        dense.zero_grad(set_to_none=True)
        value, touched = fit.backward_batch(net, samples, data, ids, a.microbatch, "wdl", scale, 0)
        emb.step()
        dense.step()
        clamp_parameters(net, touched)
        return value

    step(order[: a.batch])
    setup_s = time.perf_counter() - setup_started
    profiler = cProfile.Profile()
    profile = {}
    targets = dict(
        feature_packing=fit.batch,
        forward=net.forward,
        objective=getattr(fit, "training_loss", fit.losses),
        backward=torch.Tensor.backward,
        coalesce=None,
        zero_grad=torch.optim.Optimizer.zero_grad,
        sparse_step=torch.optim.SGD.step,
        dense_step=torch.optim.Adam.step,
        clamp=clamp_parameters,
    )
    with tempfile.TemporaryDirectory(prefix="nnue-benchmark-artifacts-") as temporary:
        work_started = time.perf_counter()
        if a.profile_worker:
            profiler.enable()
        started = time.perf_counter()
        losses = [step(order[i : i + a.batch]) for i in range(a.batch, len(order), a.batch)]
        train_s = time.perf_counter() - started
        if a.profile_worker:
            profiler.disable()
            profile["training"] = callable_profile(profiler, targets)
            profiler.clear()
        if a.profile_worker:
            profiler.enable()
        started = time.perf_counter()
        metrics = fit.evaluate(net, samples, data, validation, a.batch, "wdl", scale, 0)
        validation_s = time.perf_counter() - started
        if a.profile_worker:
            profiler.disable()
            profile["validation"] = callable_profile(
                profiler, {"feature_packing": fit.batch, "forward": net.forward, "evaluate": fit.evaluate}
            )
        artifacts = {}
        if a.artifacts:
            directory = Path(temporary)
            started = time.perf_counter()
            fit.export(net, directory / "model.bin", schema)
            artifacts["export_s"] = time.perf_counter() - started
            started = time.perf_counter()
            torch.save(
                dict(format="nnue-benchmark-v1", net=net.state_dict(), emb=emb.state_dict(), dense=dense.state_dict()),
                directory / "checkpoint.tmp",
            )
            (directory / "checkpoint.tmp").replace(directory / "checkpoint.pt")
            artifacts["checkpoint_s"] = time.perf_counter() - started
            artifacts["export_bytes"] = (directory / "model.bin").stat().st_size
            artifacts["checkpoint_bytes"] = (directory / "checkpoint.pt").stat().st_size
        workload_s = time.perf_counter() - work_started
    # These diagnostics are outside all workload timing, including validation.
    scores = []
    with torch.no_grad():
        for i in range(0, len(validation), a.batch):
            x, offsets, _ = fit.batch(samples, data, validation[i : i + a.batch])
            scores.extend((net(x, offsets) * 1000).tolist())
    sha = hashlib.sha256()
    for param in net.parameters():
        sha.update(memoryview(param.detach().numpy()))
    source_roots = [
        Path(a.module_dir).resolve(),
        Path(__file__).resolve().parent,
        Path(__file__).resolve().parents[2] / "deploy",
    ]
    sources = {}
    for name, module in list(sys.modules.items()):
        file = getattr(module, "__file__", None)
        if file and any(Path(file).resolve().is_relative_to(root) for root in source_roots):
            sources[name] = dict(path=str(file), sha256=digest(file))
    return dict(
        train_s=train_s,
        validation_s=validation_s,
        workload_s=workload_s,
        setup_s=setup_s,
        peak_rss_kib=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
        weights_sha256=sha.hexdigest(),
        losses=losses,
        metrics=metrics,
        validation_scores=scores,
        sample_ids_sha256=hashlib.sha256(json.dumps([order, validation]).encode()).hexdigest(),
        dataset_manifest_sha256=hashlib.sha256(manifest).hexdigest(),
        train_samples=a.steps * a.batch,
        validation_samples=len(validation),
        batch=a.batch,
        microbatch=a.microbatch,
        profiled=a.profile_worker,
        profile=profile,
        torch=torch.__version__,
        numpy=np.__version__,
        cpu=a.cpu,
        loaded_sources=sources,
        **artifacts,
    )
