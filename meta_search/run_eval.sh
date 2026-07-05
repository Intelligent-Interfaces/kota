#!/usr/bin/env bash
# run_eval.sh
# Kicks off the Meta-Harness optimization loop for Kota on the Forest Dew Computing benchmark.

set -e

echo "=========================================================="
echo " Starting Meta-Harness Search for Kota"
echo " Benchmark: Forest Dew Computing (Topological Resilience)"
echo "=========================================================="

# Ensure we are in the meta_search directory
cd "$(dirname "$0")"

# To actually run the search, we would invoke the meta_harness proposer.
# Assuming you have the Claude API key set up as ANTHROPIC_API_KEY.
if [ -z "$ANTHROPIC_API_KEY" ]; then
    echo "Warning: ANTHROPIC_API_KEY is not set. Meta-Harness requires it to propose mutations."
    echo "You can set it via: export ANTHROPIC_API_KEY='your-key-here'"
fi

# We call the python script. Since kota_domain.py inherits from MetaHarness,
# we need to make sure the entrypoint handles the args properly, 
# but for now we just execute it directly.
python kota_domain.py
