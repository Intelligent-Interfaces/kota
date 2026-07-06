#!/usr/bin/env python3
import os
import sys

# Add parent directory and meta_search to sys.path
sys.path.append(os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
sys.path.append(
    os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "meta_search"))
)

from meta_search.meta_harness import MetaHarness


def main():
    print("Generating agent-native documentation...")
    harness = MetaHarness()
    # Ensure workdir is set to the repo root
    harness.workdir = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))

    out_file = harness.index_for_agents(output_path="AGENTS.md")
    print(f"Successfully generated agent context map at: {out_file}")


if __name__ == "__main__":
    main()
