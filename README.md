# vyges-remap

A **Loom engine** for **file-level multi-output technology re-mapping** — the "enhanced
ABC" capability ([Antmicro's techmapping work](https://antmicro.com/blog/2026/06/multi-output-techmapping-in-openroad))
as a standalone step in a synthesis / place-and-route flow.

`vyges-remap` wraps the `vyges-emap` driver (mockturtle `emap`): given an **AIGER netlist +
a technology genlib**, it runs a single-output baseline and a multi-output pass, writes the
remapped Verilog netlist, and reports the **before/after cell & area delta** — including how
many multi-output cells (adders, compressors) were mapped that a single-output mapper can't.

## Usage

```sh
# from RTL — Yosys extracts the AIG:
vyges-remap emap --verilog design.v --top design --genlib tech.genlib -o design.remap.v --json
# or from a pre-made AIGER netlist:
vyges-remap emap --aig design.aig --genlib tech.genlib -o design.remap.v --json
```

```jsonc
{ "status": "ok", "top": "design",
  "before": { "cell_count": 15017, "cell_area": 41122, "multioutput_gates": 0 },
  "after":  { "cell_count": 13819, "cell_area": 38792, "multioutput_gates": 1171 },
  "delta":  { "cell_area": { "pct": -5.67, … }, "cell_count": { "pct": -7.98, … } },
  "multioutput_cells": 1171, "out_netlist": "design.remap.v" }
```

- `--liberty <lib>` instead of `--genlib` converts a Liberty to a genlib via ABC.
- Without `--json`, prints a short human summary.

> Multi-output cells are derived by the tech library from single-output genlib gates that
> share inputs (e.g. `xor3`+`maj3` → full-adder). A genlib **without** those (no XOR) yields
> zero multi-output cells — the reduction scales with the design's arithmetic content.

## Dependencies

- **`vyges-emap`** (the mockturtle emap driver) — resolved via `$VYGES_EMAP` (default
  `vyges-emap` on `PATH`). Ships from `vyges-tools/vyges-mockturtle`.
- **Yosys** — for `--verilog` (RTL → AIG), resolved via `$VYGES_YOSYS` (default `yosys`).
- **ABC** — only for `--liberty` (Liberty→genlib), resolved via `$VYGES_ABC` (default `abc`).

## As a Loom engine

`vyges-remap --describe` publishes a structured contract, so `vyges mcp` advertises it as a
typed tool (AIGER + genlib in → the before/after delta + remapped netlist out) that an agent
can drive. Discoverable once installed (`vyges install loom`).

## Licensing

Apache-2.0 (`LICENSE`). It invokes `vyges-emap` (MIT) and ABC as separate processes — no
GPL/AGPL code is linked in.
