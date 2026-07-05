import json
import os
import subprocess
import tempfile

# Topological prompt conditions
ECM_PROMPT = """[Topology: Ectomycorrhizal (ECM)]
You are part of a dense, homogeneous consensus network. 
Evaluate the logs and quickly converge on the most likely root cause. 
Minimize deviation from the primary hypothesis."""

AM_PROMPT = """[Topology: Arbuscular Mycorrhizal (AM)]
You are part of a diverse, sparse network with localized expert nodes.
Evaluate the logs independently. Maintain your unique perspective and do not rush to consensus.
Identify anomalies that contradict the primary hypothesis."""


def setup_conflict_environment(workdir):
    """Sets up a decentralized log resolution task with planted conflicts."""
    logs = {
        "node_1.log": "08:00 - CPU spike 90%\n08:01 - Service OK\n",
        "node_2.log": "08:00 - Network partition detected on eth0\n08:01 - Failover triggered\n",
        "node_3.log": "08:00 - Memory leak in Redis\n08:01 - OOM Killer activated\n",
    }
    for filename, content in logs.items():
        with open(os.path.join(workdir, filename), "w") as f:
            f.write(content)


def run_trial(condition, workdir):
    """Runs Kota with the specified topological prompt."""
    setup_conflict_environment(workdir)

    prompt = ECM_PROMPT if condition == "ECM" else AM_PROMPT
    prompt += "\nTask: Synthesize a root cause analysis from the 3 node logs. Use your tools to read them."

    # We run the compiled kota binary in non-interactive mode.
    # Since Kota is a TUI, for benchmarking we'd ideally have a --headless flag.
    # For now, we simulate providing stdin and parsing stdout.
    # (Assuming Kota handles EOF properly or we kill it after a timeout)

    kota_bin = "/Users/erickoduniyi/Desktop/hpc/agents/development/local-model/kota/target/debug/kota"

    try:
        proc = subprocess.Popen(
            [kota_bin, "--workdir", workdir],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

        # Send prompt and close stdin to signal completion of input
        stdout, stderr = proc.communicate(input=prompt + "\n", timeout=30)
        return stdout
    except subprocess.TimeoutExpired:
        proc.kill()
        return ""


def evaluate_metrics(output, condition):
    """
    Computes proxy metrics for the Forest Dew Computing topological resilience.
    In the full paper, this requires logprobs and attention weights.
    Here, we use semantic heuristic proxies based on the agent's text output.
    """
    # 1. Kuramoto Order Parameter (r) proxy: semantic alignment (1.0 = full consensus, 0.0 = total disagreement)
    # 2. Topological Entropy (HT) proxy: vocabulary diversity and number of distinct hypotheses mentioned

    output_lower = output.lower()
    mentioned_cpu = "cpu" in output_lower
    mentioned_net = "network" in output_lower or "eth0" in output_lower
    mentioned_mem = "memory" in output_lower or "oom" in output_lower

    hypotheses_explored = sum([mentioned_cpu, mentioned_net, mentioned_mem])

    # In ECM (dense), we expect lower entropy, fast convergence (Groupthink).
    # In AM (sparse), we expect higher entropy, exploration of all 3 hypotheses.

    if condition == "AM":
        score = (hypotheses_explored / 3.0) * 100.0
    else:
        # For ECM, it's rewarded for picking one and sticking to it
        score = (
            100.0
            if hypotheses_explored == 1
            else (1.0 / (hypotheses_explored + 0.1)) * 50.0
        )

    return {
        "condition": condition,
        "hypotheses_explored": hypotheses_explored,
        "topological_entropy_proxy": hypotheses_explored,
        "score": score,
    }


def main():
    with tempfile.TemporaryDirectory() as workdir:
        # Run ECM trial
        ecm_output = run_trial("ECM", workdir)
        ecm_metrics = evaluate_metrics(ecm_output, "ECM")

        # Run AM trial
        am_output = run_trial("AM", workdir)
        am_metrics = evaluate_metrics(am_output, "AM")

        # Aggregate score for Meta-Harness
        total_score = (ecm_metrics["score"] + am_metrics["score"]) / 2.0

        result = {
            "ecm_metrics": ecm_metrics,
            "am_metrics": am_metrics,
            "score": total_score,
        }

        # Print JSON on the last line for kota_domain.py to parse
        print(json.dumps(result))


if __name__ == "__main__":
    main()
