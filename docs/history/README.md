# History

The finished specs and plans. They are history and never a specification: every rule that still
holds lives in `AGENTS.md`, `.agents/rules/ARCHITECTURE.md` and `docs/` (`stats.md`,
`invariants.md`, `known_limitations.md`). Do not read or search this folder unless the user or
the current spec points to a section. Files keep their names, so a reference such as
"SPEC_v3.7 §6" in a code comment names a file here.

| Milestones | Version | File | What it still records |
| --- | --- | --- | --- |
| M0–M5 | v3.5 | `BLUEPRINT.md` (frozen v3.5 reference implementation, read by `tools/blueprint`; never retype its code blocks) and `TASKS_v3.5.md` (its milestone plan) | The original design and code |
| M6–M11 | v3.6 | `SPEC_v3.6.md` | GGPoker heads-up, shows, straddles, EV Cashout |
| M12 | v3.6.1 | `SPEC_v3.6.1.md` | Audit fixes |
| M13–M16 | v3.7 | `SPEC_v3.7.md` | The derived-statistics layer. §4–§7 pin the corpus numbers, which the tests, the goldens and `tools/oracles/v3_7_oracle.py` reproduce (its output is labelled by these sections). §10 has the product decisions with their owners and reasons; their outcomes are in the living docs. §11–§12 are its amendments |
| M16.1–M16.2 | v3.7.1 | `SPEC_v3.7.1.md` | Fixes from the v3.7 review; §3 has owner decisions D1–D3 |

`docs/CHANGELOG.md` summarises every version.

When a version is complete, its spec moves here unchanged. Any contract that still holds is
first copied into the living docs.
