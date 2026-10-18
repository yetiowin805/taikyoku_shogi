"""Persistent partial minimax tree; engine hints are never treated as game proofs."""

import math


def key(move):
    import json

    return json.dumps(move, sort_keys=True, separators=(",", ":"))


class Tree:
    def __init__(self, actual, spine_length=9, data=None):
        self.data = data or dict(nodes=[], actual=actual, spine_length=spine_length, actions=0, cursor=0)
        self.nodes = self.data["nodes"]
        if not self.nodes:
            self.add(None, None)

    def add(self, parent, move):
        if parent is not None:
            k = key(move)
            if k in self.nodes[parent]["children"]:
                return self.nodes[parent]["children"][k]
        number = len(self.nodes)
        path = [] if parent is None else self.nodes[parent]["path"] + [move]
        node = dict(
            id=number,
            parent=parent,
            path=path,
            children={},
            legal=None,
            terminal=None,
            turn=1,
            static=0,
            records={},
            candidates=[],
            best=None,
            searches=0,
            requested=0,
            failed_attempts=0,
            last_cost=0,
            reply=None,
        )
        self.nodes.append(node)
        if parent is not None:
            self.nodes[parent]["children"][key(move)] = number
        return number

    def inspect(self, number, result):
        node = self.nodes[number]
        node.update(
            legal=result["legal"],
            terminal=result["terminal"],
            turn=result["turn"],
            static=result["static_black"],
            reply=result.get("reply"),
        )
        node["position_hash"] = result["position_hash"]
        node["history_len"] = result["history_len"]
        node["draw_counter"] = result["draw_counter"]
        node["repetitions"] = result["repetitions"]

    def iteration(self, number, result):
        node = self.nodes[number]
        node["records"][str(result["depth"])] = result
        deepest = max(map(int, node["records"]))
        record = node["records"][str(deepest)]
        node.update(best=record["best"], candidates=record["candidates"])

    def spine(self):
        result = [0]
        for move in self.data["actual"][: self.data["spine_length"]]:
            result.append(self.add(result[-1], move))
        return result

    def depth(self, number):
        return max(map(int, self.nodes[number]["records"]), default=0)

    def parity(self, number):
        records = self.nodes[number]["records"]
        odd = [int(d) for d in records if int(d) % 2]
        even = [int(d) for d in records if not int(d) % 2]
        if odd and even:
            return abs(records[str(max(odd))]["score_black"] - records[str(max(even))]["score_black"])
        return 0

    def backup(self):
        """Separate partial-tree estimates from exact WDL intervals over *all* legal moves."""
        for node in reversed(self.nodes):
            if node["terminal"] is not None:
                value = node["terminal"]
                node.update(
                    value=value * 1000000, proof_low=value, proof_high=value, selected=None, spread=0, options=[]
                )
                continue
            choose = max if node["turn"] == 1 else min
            depth = self.depth(node["id"])
            base = node["records"][str(depth)]["score_black"] if depth else node["static"]
            # Shadow candidates retain explicitly heuristic reduced/PVS scores until independently searched.
            options = {
                key(c["move"]): dict(move=c["move"], value=c["score_black"], shadow=True) for c in node["candidates"]
            }
            for k, child_id in node["children"].items():
                child = self.nodes[child_id]
                if child["records"] or child["terminal"] is not None:
                    options[k] = dict(move=child["path"][-1], value=child["value"], shadow=False)
            if options:
                best = choose(options.values(), key=lambda c: c["value"])
                node.update(value=best["value"], selected=best["move"])
            else:
                node.update(value=base, selected=node["best"])
            node["spread"] = max(self.parity(node["id"]), abs(base - node["value"]))
            node["options"] = sorted(options.values(), key=lambda c: -node["turn"] * c["value"])
            bounds = [
                (self.nodes[c].get("proof_low", -1), self.nodes[c].get("proof_high", 1))
                for c in node["children"].values()
            ]
            # No unseen move is discarded because its search hint was poor.
            if node["legal"] is None or len(node["children"]) < len(node["legal"]):
                bounds.append((-1, 1))
            node["proof_low"] = choose((b[0] for b in bounds), default=-1)
            node["proof_high"] = choose((b[1] for b in bounds), default=1)

    def expand(self, number, move, reason):
        return dict(node=self.add(number, move), kind="search", reason=reason, depth=3)

    def coverage(self):
        """Fair breadth-first expansion is a completeness fallback, not a practical solving claim."""
        for node in sorted(self.nodes, key=lambda n: (len(n["path"]), n["id"])):
            if node["legal"] is None:
                return dict(node=node["id"], kind="inspect", reason="coverage")
            if node["terminal"] is None:
                for item in node["legal"]:
                    if key(item["move"]) not in node["children"]:
                        return dict(node=self.add(node["id"], item["move"]), kind="inspect", reason="coverage")
        return None

    def descend(self, start):
        number = start
        while True:
            node = self.nodes[number]
            if node["terminal"] is not None:
                return None
            move = node.get("selected")
            if move is None:
                return dict(node=number, kind="search", reason="pv-leaf", depth=max(3, self.depth(number) + 1))
            child = node["children"].get(key(move))
            if child is None or (not self.nodes[child]["records"] and self.nodes[child]["terminal"] is None):
                return self.expand(number, move, "pv-descent")
            number = child

    def contested(self, spine):
        candidates = []
        for n in self.nodes:
            if n["terminal"] is not None or not n["records"]:
                continue
            gap = abs(n["options"][0]["value"] - n["options"][1]["value"]) if len(n["options"]) > 1 else 1000
            priority = (400 + n["spread"] + 1000000 / (1000 + gap)) / math.sqrt(1 + n["searches"])
            priority /= 1 + 0.15 * len(n["path"])
            candidates.append((priority, n["id"]))
        for _, number in sorted(candidates, reverse=True):
            node = self.nodes[number]
            if self.data["actions"] % 3 == 0:
                return dict(node=number, kind="search", reason="instability-deepen", depth=self.depth(number) + 1)
            for option in node["options"][:3]:
                child = node["children"].get(key(option["move"]))
                if child is None or (not self.nodes[child]["records"] and self.nodes[child]["terminal"] is None):
                    return self.expand(number, option["move"], "challenger")
            result = self.descend(number)
            if result:
                return result
        return self.descend(spine[self.data["cursor"] % len(spine)])

    def refutation(self, spine):
        actual = self.data["actual"]
        for offset in range(len(spine) - 1):
            i = (self.data["cursor"] + offset) % (len(spine) - 1)
            if i + 1 >= len(actual):
                continue
            n = self.nodes[spine[i]]
            reply = actual[i + 1]
            for entry in n["legal"] or []:
                mv = entry["move"]
                # Inexpensive nominations: save the reply's destination victim, capture its mover,
                # or block its straight corridor. Full legal response/search always follows.
                a, b, p = reply["from"], reply["to"], mv["to"]
                cross = (p["file"] - a["file"]) * (b["rank"] - a["rank"]) == (p["rank"] - a["rank"]) * (
                    b["file"] - a["file"]
                )
                between = (
                    cross
                    and min(a["file"], b["file"]) <= p["file"] <= max(a["file"], b["file"])
                    and min(a["rank"], b["rank"]) <= p["rank"] <= max(a["rank"], b["rank"])
                )
                if (mv["from"] == reply["to"] or mv["to"] == reply["from"] or between) and key(mv) not in n["children"]:
                    action = self.expand(n["id"], mv, "known-response-defense")
                    action["reply"] = reply
                    return action
        return None

    def action(self, strategy):
        spine = self.spine()
        self.backup()
        # The same finite-cost fair coverage step is present in all four strategies.
        if self.data["actions"] % 10 == 9:
            return self.coverage()
        self.data["cursor"] += 1
        if strategy == "local":
            for i in range(len(spine)):
                n = spine[(self.data["cursor"] - 1 + i) % len(spine)]
                if self.nodes[n]["terminal"] is None:
                    return dict(node=n, kind="search", reason="played-line-deepen", depth=self.depth(n) + 1)
            return self.coverage()
        if strategy == "pv":
            for i in range(len(spine)):
                a = self.descend(spine[(self.data["cursor"] + i) % len(spine)])
                if a:
                    return a
        if strategy == "refutation" and self.data["actions"] % 3 != 0:
            a = self.refutation(spine)
            if a:
                return a
        return self.contested(spine) or self.coverage()

    def diagnosis(self, start_ply):
        spine = self.spine()
        self.backup()
        result = []
        for number, child in zip(spine, spine[1:], strict=False):
            n, c = self.nodes[number], self.nodes[child]
            result.append(
                dict(
                    ply=start_ply + len(n["path"]),
                    value_black=n["value"],
                    played_value_black=c["value"],
                    regret=max(0, n["turn"] * (n["value"] - c["value"])),
                    best=n["selected"],
                    played=c["path"][-1],
                    parity_spread=self.parity(number),
                    proof=[n["proof_low"], n["proof_high"]],
                    depth=self.depth(number),
                    child_depth=self.depth(child),
                    node=number,
                )
            )
        return sorted(result, key=lambda x: (-x["regret"], x["ply"]))
