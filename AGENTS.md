# Agent Context Map

This repository has been indexed for agent consumption. Use this map to quickly grasp the project structure and primary module purposes.

## Directory Tree

  - AGENTS.md
  - Cargo.lock
  - Cargo.toml
  - README.md
  - add_two_numbers.go
  - audit.toml
  - book.toml
  - boston_mcp.py
  - generate_table.py
  - mcp_config.json
  - output.md
  - ruff.toml
  - test_tui_md.rs
  - go/
    - go.mod
    - search_wiki/
      - search_wiki.go
      - search_wiki_test.go
    - watchdog/
      - watchdog.go
      - watchdog_test.go
    - coordinator/
      - coordinator.go
  - scripts/
    - generate_agent_docs.py
  - src/
    - agent.rs
    - events.rs
    - index.html
    - llm.rs
    - main.rs
    - mcp.rs
    - memory.rs
    - power.rs
    - sensing.rs
    - server.rs
    - skills.rs
    - telemetry.rs
    - tools.rs
    - tui.rs
    - vertex.rs
  - meta_search/
    - anthropic_caching.py
    - claude_wrapper.py
    - forest_dew_benchmark.py
    - kota_domain.py
    - meta_harness.py
    - run_eval.sh
    - logs/
      - evolution_summary.jsonl
  - notebooks/
    - L1_Kota_Architecture.ipynb
    - L2_Memory_and_Turso.ipynb
    - L3_Agent_Skills_and_HITL.ipynb
    - L4_Observability_and_Metrics.ipynb
    - L5_Cognitive_Sensing.ipynb
    - L6_Quality_Workflows.ipynb
    - Pipfile
    - requirements.txt

## Module Documentation

### `meta_search/claude_wrapper.py`

```text
Minimal wrapper around `claude -p` for programmatic usage with logging.
Calls Claude Code CLI via subprocess, parses stream-json output,
tracks tool calls / file reads / token usage, and logs everything to disk.
Works independently of your local Claude Code setup (skills/plugins not inherited)
```

### `meta_search/meta_harness.py`

```text
Autonomous evolution loop for agent scaffolds on Terminal-Bench 2.0.

Starts from the shipped KIRA baseline and evolves improvements on the full
official TB2 dataset used in the paper runs.

    uv run python meta_harness.py --iterations 5
    uv run python meta_harness.py --iterations 10 --trials 2 --fresh
```
