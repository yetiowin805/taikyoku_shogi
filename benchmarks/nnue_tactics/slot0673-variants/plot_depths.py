#!/usr/bin/env python3
"""Plot completed-depth scores from the deeper target searches; no engine runs."""
import json
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

HERE = Path(__file__).resolve().parent
summary = json.loads((HERE / 'summary.json').read_text())
fig, axes = plt.subplots(1, 2, figsize=(11, 4.6), sharey=True)
for ax, target in zip(axes, ['target174', 'target176']):
    for variant, color, label in [('baseline', '#444444', 'Baseline'),
                                  ('tempo_50', '#007a83', 'Half turn component'),
                                  ('tempo_0', '#8254a3', 'Zero turn component')]:
        matches = [r for r in summary['runs'] if r['position'] == target and r['variant'] == variant
                   and r['mode'] == 'fixed' and r['returncode'] == 0]
        row = max(matches, key=lambda r: r['completed_depth'])
        iterations = row['iterations']
        ax.plot([e['completed_depth'] for e in iterations], [-e['score'] for e in iterations],
                'o-', color=color, label=label, linewidth=1.8, markersize=5)
    ax.axhline(0, color='#999999', linewidth=.7)
    ax.set_title(f'Before White ply {target[6:]}', fontsize=12)
    ax.set_xlabel('Completed search depth (plies)')
    ax.set_xticks(range(1, 6))
    ax.spines[['top', 'right']].set_visible(False)
    ax.grid(axis='y', color='#eeeeee')
axes[0].set_ylabel('Search score (evaluation units; positive for White)')
axes[1].legend(frameon=False, loc='best')
fig.suptitle('Turn-component damping reduces score oscillation in these two positions', fontsize=13)
fig.text(.02, .02, 'Source: slot 673, frozen W512 v4.5, deeper fixed-depth probes. Different evaluation calibrations; scores are not ground truth.', fontsize=8)
fig.tight_layout(rect=[0, .06, 1, .93])
fig.savefig(HERE / 'depth-scores.png', dpi=160)
