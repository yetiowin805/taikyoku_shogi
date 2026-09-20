"""Serializable stopping policy; validation Huber is the monitored metric."""
from dataclasses import asdict, dataclass
import math


@dataclass(frozen=True)
class Policy:
    min_epochs: int = 8
    max_epochs: int = 32
    patience: int = 3
    min_improvement: float = 0.005
    lr_factor: float = 0.5
    lr_reductions: int = 2

    def __post_init__(self):
        integers = (self.min_epochs, self.max_epochs, self.patience, self.lr_reductions)
        if not (all(type(value) is int for value in integers)
                and 1 <= self.min_epochs <= self.max_epochs and self.patience >= 1
                and 0 < self.min_improvement < 1 and 0 < self.lr_factor < 1
                and self.lr_reductions >= 0):
            raise ValueError("Invalid plateau policy")


@dataclass
class Plateau:
    best: float
    bad_epochs: int = 0
    reductions: int = 0
    stopped: str | None = None

    def observe(self, epoch, loss, policy):
        if not math.isfinite(loss):
            raise ValueError("Nonfinite validation loss")
        if self.stopped:
            return self.stopped
        if loss < self.best * (1 - policy.min_improvement):
            self.best, self.bad_epochs = loss, 0
        else:
            self.bad_epochs += 1
        if epoch >= policy.max_epochs:
            self.stopped = "max_epochs"
        elif epoch >= policy.min_epochs and self.bad_epochs >= policy.patience:
            self.bad_epochs = 0
            if self.reductions < policy.lr_reductions:
                self.reductions += 1
                return "reduce_lr"
            self.stopped = "plateau"
        return self.stopped or "continue"

    def record(self, policy):
        return {"policy": asdict(policy), "state": asdict(self)}
