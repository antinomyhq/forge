#!/usr/bin/env python3
"""
SWE-bench Pro evaluation wrapper for Forge-generated patches.

This script:
1. Clones the SWE-bench_Pro-os evaluation repository (if needed)
2. Invokes the official swe_bench_pro_eval.py with the correct arguments
3. Parses and displays evaluation results

Usage:
    python3 swebench/evaluate.py \
        --patches-path results/forge-sonnet/patches.json \
        --output-dir results/forge-sonnet/eval \
        --num-workers 10

Prerequisites:
    - Docker must be running
    - pip install docker pandas tqdm
    - (Optional) pip install modal  -- for Modal-based eval
"""

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

SWEBENCH_PRO_REPO = "https://github.com/scaleapi/SWE-bench_Pro-os.git"
DEFAULT_CLONE_DIR = Path(__file__).resolve().parent / ".swe-bench-pro-os"
DEFAULT_DOCKERHUB_USERNAME = "jefzda"
DEFAULT_NUM_WORKERS = 10


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def clone_or_update_repo(repo_path: Path) -> None:
    """Clone the SWE-bench_Pro-os repository, or pull latest if it exists."""
    if (repo_path / ".git").exists():
        print(f"SWE-bench Pro repo already exists at {repo_path}, pulling latest...")
        subprocess.run(
            ["git", "pull", "--ff-only"],
            cwd=str(repo_path),
            check=False,
            capture_output=True,
        )
    else:
        print(f"Cloning SWE-bench_Pro-os to {repo_path}...")
        repo_path.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            ["git", "clone", "--depth", "1", SWEBENCH_PRO_REPO, str(repo_path)],
            check=True,
        )
    print(f"SWE-bench Pro repo ready at {repo_path}")


def find_eval_script(repo_path: Path) -> Path:
    """Locate the swe_bench_pro_eval.py script inside the cloned repo."""
    candidates = [
        repo_path / "swe_bench_pro_eval.py",
        repo_path / "sweap_pro_eval_modal.py",
    ]
    for candidate in candidates:
        if candidate.exists():
            return candidate
    raise FileNotFoundError(
        f"Could not find evaluation script in {repo_path}.\n"
        f"Tried: {', '.join(str(c) for c in candidates)}"
    )


def find_raw_sample(repo_path: Path) -> Path:
    """Locate the raw sample data file (JSONL or CSV)."""
    candidates = [
        repo_path / "helper_code" / "sweap_eval_full_v2.jsonl",
        repo_path / "helper_code" / "sweap_eval_full.jsonl",
        repo_path / "helper_code" / "sweap_eval_full_v2.csv",
        repo_path / "data.csv",
    ]
    for candidate in candidates:
        if candidate.exists():
            return candidate
    raise FileNotFoundError(
        f"Could not find raw sample data in {repo_path}.\n"
        f"Tried: {', '.join(str(c) for c in candidates)}"
    )


def display_results(eval_results_path: Path) -> None:
    """Load and display evaluation results from eval_results.json."""
    if not eval_results_path.exists():
        print("Warning: eval_results.json not found")
        return

    with open(eval_results_path) as f:
        results = json.load(f)

    total = len(results)
    passed = sum(1 for v in results.values() if v)
    failed = total - passed
    accuracy = (passed / total * 100) if total > 0 else 0.0

    print(f"\n{'=' * 60}")
    print(f"  SWE-bench Pro Evaluation Results")
    print(f"{'=' * 60}")
    print(f"  Total instances evaluated: {total}")
    print(f"  Passed:                    {passed}")
    print(f"  Failed:                    {failed}")
    print(f"  Accuracy:                  {accuracy:.2f}%")
    print(f"{'=' * 60}")

    # Show per-instance results
    if total <= 50:
        print("\nPer-instance results:")
        for instance_id, result in sorted(results.items()):
            status = "PASS" if result else "FAIL"
            print(f"  [{status}] {instance_id}")
    else:
        # Show failures only for large result sets
        failures = [iid for iid, v in results.items() if not v]
        if failures:
            print(f"\nFailed instances ({len(failures)}):")
            for iid in sorted(failures)[:20]:
                print(f"  [FAIL] {iid}")
            if len(failures) > 20:
                print(f"  ... and {len(failures) - 20} more")


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def parse_args() -> argparse.Namespace:
    """Parse command-line arguments."""
    parser = argparse.ArgumentParser(
        description="Evaluate Forge-generated patches using SWE-bench Pro",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Evaluate patches using local Docker
  python swebench/evaluate.py \\
      --patches-path results/forge-sonnet/patches.json \\
      --output-dir results/forge-sonnet/eval

  # Use Modal for evaluation (requires modal setup)
  python swebench/evaluate.py \\
      --patches-path results/forge-sonnet/patches.json \\
      --output-dir results/forge-sonnet/eval \\
      --use-modal

  # Use a custom SWE-bench Pro checkout
  python swebench/evaluate.py \\
      --patches-path results/forge-sonnet/patches.json \\
      --output-dir results/forge-sonnet/eval \\
      --swebench-repo /path/to/SWE-bench_Pro-os
""",
    )

    parser.add_argument(
        "--patches-path",
        required=True,
        help="Path to patches.json from run_forge.py",
    )
    parser.add_argument(
        "--output-dir",
        required=True,
        help="Evaluation output directory",
    )
    parser.add_argument(
        "--num-workers",
        type=int,
        default=DEFAULT_NUM_WORKERS,
        help=f"Parallel eval workers (default: {DEFAULT_NUM_WORKERS})",
    )
    parser.add_argument(
        "--swebench-repo",
        default=None,
        help=f"Path to cloned SWE-bench_Pro-os (default: auto-clone to {DEFAULT_CLONE_DIR})",
    )
    parser.add_argument(
        "--use-modal",
        action="store_true",
        help="Use Modal instead of local Docker for evaluation",
    )
    parser.add_argument(
        "--dockerhub-username",
        default=DEFAULT_DOCKERHUB_USERNAME,
        help=f"DockerHub username (default: {DEFAULT_DOCKERHUB_USERNAME})",
    )
    parser.add_argument(
        "--docker-platform",
        default=None,
        help="Docker platform override (e.g., linux/amd64)",
    )
    parser.add_argument(
        "--redo",
        action="store_true",
        help="Re-evaluate even if output exists",
    )
    parser.add_argument(
        "--block-network",
        action="store_true",
        help="Block network access inside evaluation containers",
    )

    return parser.parse_args()


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main() -> None:
    args = parse_args()

    patches_path = Path(args.patches_path).resolve()
    output_dir = Path(args.output_dir).resolve()
    output_dir.mkdir(parents=True, exist_ok=True)

    # Validate patches file
    if not patches_path.exists():
        print(f"Error: patches file not found: {patches_path}")
        sys.exit(1)

    with open(patches_path) as f:
        patches = json.load(f)
    print(f"Loaded {len(patches)} patches from {patches_path}")

    if not patches:
        print("No patches to evaluate.")
        return

    # Clone or locate SWE-bench Pro repo
    repo_path = Path(args.swebench_repo).resolve() if args.swebench_repo else DEFAULT_CLONE_DIR
    clone_or_update_repo(repo_path)

    # Locate required files
    eval_script = find_eval_script(repo_path)
    raw_sample = find_raw_sample(repo_path)
    scripts_dir = repo_path / "run_scripts"

    if not scripts_dir.exists():
        print(f"Error: run_scripts directory not found at {scripts_dir}")
        print("The SWE-bench_Pro-os repo may be incomplete or has a different structure.")
        sys.exit(1)

    print(f"Eval script:  {eval_script}")
    print(f"Raw sample:   {raw_sample}")
    print(f"Scripts dir:  {scripts_dir}")

    # Build the evaluation command
    cmd = [
        sys.executable,
        str(eval_script),
        f"--raw_sample_path={raw_sample}",
        f"--patch_path={patches_path}",
        f"--output_dir={output_dir}",
        f"--scripts_dir={scripts_dir}",
        f"--num_workers={args.num_workers}",
        f"--dockerhub_username={args.dockerhub_username}",
    ]

    if not args.use_modal:
        cmd.append("--use_local_docker")

    if args.redo:
        cmd.append("--redo")

    if args.block_network:
        cmd.append("--block_network")

    if args.docker_platform:
        cmd.append(f"--docker_platform={args.docker_platform}")

    print(f"\nRunning evaluation command:")
    print(f"  {' '.join(cmd)}\n")

    # Run the evaluation
    env = os.environ.copy()
    # Add the repo to PYTHONPATH so helper_code imports work
    existing_pythonpath = env.get("PYTHONPATH", "")
    env["PYTHONPATH"] = f"{repo_path}:{existing_pythonpath}" if existing_pythonpath else str(repo_path)

    result = subprocess.run(cmd, cwd=str(repo_path), env=env)

    if result.returncode != 0:
        print(f"\nEvaluation exited with code {result.returncode}")
        # Still try to display any results that were generated
    
    # Display results
    eval_results_path = output_dir / "eval_results.json"
    display_results(eval_results_path)


if __name__ == "__main__":
    main()
