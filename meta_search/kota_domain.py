import subprocess
import json
from meta_harness import MetaHarness

class KotaForestDewDomain(MetaHarness):
    """
    Meta-Harness domain specification for optimizing Kota on the Forest Dew Computing benchmark.
    This optimizer rewrites Kota's Rust harness (src/agent.rs) to improve its resilience and
    coordination on complex decentralized tasks.
    """
    
    def __init__(self):
        super().__init__()
        self.target_files = ["src/agent.rs", "src/tools.rs"]
        self.workdir = "/Users/erickoduniyi/Desktop/hpc/agents/development/local-model/kota"

    def compile_kota(self):
        print("Compiling Kota...")
        result = subprocess.run(
            ["cargo", "build"],
            cwd=self.workdir,
            capture_output=True,
            text=True
        )
        if result.returncode != 0:
            return False, result.stderr
        return True, ""

    def evaluate_candidate(self, candidate_path):
        """
        Evaluate the modified Kota agent on the Forest Dew Computing benchmark.
        """
        # 1. Compile the candidate
        success, err = self.compile_kota()
        if not success:
            print(f"Compilation failed:\n{err}")
            return 0.0, f"Compilation failed: {err}"

        # 2. Run the Forest Dew benchmark Python script
        print("Running Forest Dew Benchmark...")
        result = subprocess.run(
            ["python", "meta_search/forest_dew_benchmark.py"],
            cwd=self.workdir,
            capture_output=True,
            text=True
        )

        try:
            # Benchmark script should print a JSON summary on the last line
            lines = result.stdout.strip().split('\n')
            metrics = json.loads(lines[-1])
            score = metrics.get('score', 0.0)
            return score, result.stdout
        except Exception as e:
            return 0.0, f"Benchmark parsing failed: {e}\nOutput: {result.stdout}"

if __name__ == "__main__":
    domain = KotaForestDewDomain()
    # Placeholder to run the search loop
    print("Kota Domain Initialized. Ready for Meta-Harness Optimization.")
