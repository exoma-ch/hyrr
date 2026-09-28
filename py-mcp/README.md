# hyrr-mcp

HYRR MCP server — stdio JSON-RPC for radio-isotope production queries.

Listed in the [official MCP Registry](https://registry.modelcontextprotocol.io/v0/servers?search=io.github.exoma-ch/hyrr)
as `io.github.exoma-ch/hyrr`.

<!-- This README is the PyPI long description. The registry proves we own the
     PyPI package by finding the token below in the README of the exact version
     being listed (#706), so it must ship in every release. The name must match
     py-mcp/server.json; scripts/check-mcp-registry.sh enforces that. -->
<!-- mcp-name: io.github.exoma-ch/hyrr -->

Install and register with Claude Code:

```bash
claude mcp add hyrr -- uvx hyrr-mcp
```

The first invocation downloads the wheel (or builds from source if no
wheel exists for your platform); subsequent runs are instant.

Data resolution priority: `--data-dir` arg → `HYRR_DATA` env →
sibling `nucl-parquet/` → `~/.hyrr/nucl-parquet`.

The tools surface (`simulate`, `list_materials`,
`list_reaction_channels`, `get_decay_data`, `compare_simulations`,
`get_stack_energy_budget`, `get_stopping_power`,
`get_isotope_production_curve`) is implemented in Rust in
`hyrr-core::mcp`; this package is a thin PyO3 wrapper used purely
for distribution.
