---
trigger: model_decision
description: Where poker_tracker milestones are defined and how one is run, verified and reported. Read when starting, continuing, verifying or reporting on any milestone, or when the user asks what to do next.
---

# `poker_tracker` Milestones

Milestones are defined only in the current spec. This file says where, and how a milestone is
done.

## Where

| Milestones | Version | Spec | Status |
| --- | --- | --- | --- |
| M0–M5 | v3.5 | `docs/BLUEPRINT.md` (plan archived in `docs/history/TASKS_v3.5.md`) | Done |
| M6–M11 | v3.6 | `docs/SPEC_v3.6.md` | Done |
| M12 | v3.6.1 | `docs/SPEC_v3.6.1.md` | Done |
| **M13–M16** | **v3.7** | **`docs/SPEC_v3.7.md`** | **Current** |
| M17, M19 | later | Sketched in `docs/SPEC_v3.7.md` §8 | Planned — do not build |
| M18, M20 | later | Named in `docs/SPEC_v3.7.md` §8 | Deferred |

The last completed milestone is the newest `milestone <N>:` commit (`git log --oneline`).

## How a milestone is done

1. Read its section of the current spec, and `docs/stats.md` for anything statistical.
2. Build it test-first (`AGENTS.md` §4).
3. Update the living docs it touches in the same commit (`AGENTS.md` §4).
4. Run the gate with the `milestone-gate` skill; every command must pass and the test count must
   go up (code milestone) or stay equal (docs-only milestone).
5. Run any scratch import or report the milestone's **Done when** asks for, then delete the
   scratch database.
6. Commit: `milestone <N>: <summary> (gate: <count> tests passing)`.
7. Report: the gate block, the test count, **Deviations** (or "none"), the scratch-run numbers,
   and `git show --stat HEAD`. Then wait for the user.

## Anything not in a spec

Propose a plan first (files, public API, schema changes, tests), wait for the user's approval,
then implement against the same gate.
