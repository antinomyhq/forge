#!/usr/bin/env python3
"""
SWE-bench Pro runner for the Forge CLI agent.

This script orchestrates running Forge against SWE-bench Pro instances:
1. Loads instances from HuggingFace (ScaleAI/SWE-bench_Pro)
2. Spins up Docker containers using pre-built SWE-bench Pro images
3. Installs and runs Forge inside each container to generate patches
4. Collects patches into the format expected by swe_bench_pro_eval.py

Usage:
    python3 swebench/run_forge.py \\
        --output-dir results/forge-sonnet \\
        --model anthropic/claude-sonnet-4 \\
        --slice 0:5 \\
        --num-workers 2
"""

import argparse
import concurrent.futures
import json
import os
import platform as py_platform
import re
import shlex
import time
import uuid
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import docker
from datasets import load_dataset
from tqdm import tqdm

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

DATASET_NAME = "ScaleAI/SWE-bench_Pro"
DATASET_SPLIT = "test"
DEFAULT_DOCKERHUB_USERNAME = "jefzda"
DEFAULT_PREFIX = "forge"
DEFAULT_TIMEOUT = 3600  # 1 hour
DEFAULT_NUM_WORKERS = 4
CONTAINER_APP_DIR = "/app"
CONTAINER_WORKSPACE_DIR = "/workspace"
CONTAINER_FORGE_BIN = "/usr/local/bin/forge"
CONTAINER_FORGE_CONFIG_DIR = "/root/forge"

# API key and URL environment variables to pass through to the container.
# Mirrors bench/forge_agent.py:681-728
API_KEY_VARS = [
    "FORGE_API_KEY",
    "DEEPSEEK_API_KEY",
    "GITHUB_COPILOT_API_KEY",
    "OPENROUTER_API_KEY",
    "REQUESTY_API_KEY",
    "XAI_API_KEY",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "CLAUDE_API_KEY",
    "CEREBRAS_API_KEY",
    "ZAI_API_KEY",
    "ZAI_CODING_API_KEY",
    "BIG_MODEL_API_KEY",
    "VERTEX_AI_AUTH_TOKEN",
    "AZURE_API_KEY",
    "LLAMA_CPP_API_KEY",
    "VLLM_API_KEY",
    "JAN_AI_API_KEY",
    "OLLAMA_API_KEY",
    "LM_STUDIO_API_KEY",
    "IO_INTELLIGENCE_API_KEY",
]

URL_VARS = [
    "OPENAI_URL",
    "ANTHROPIC_URL",
    "PROJECT_ID",
    "LOCATION",
    "AZURE_RESOURCE_NAME",
    "AZURE_DEPLOYMENT_NAME",
    "AZURE_API_VERSION",
    "LLAMA_CPP_URL",
    "LLAMA_CPP_PORT",
    "VLLM_URL",
    "VLLM_PORT",
    "JAN_AI_URL",
    "JAN_AI_PORT",
    "OLLAMA_URL",
    "OLLAMA_PORT",
    "LM_STUDIO_URL",
    "LM_STUDIO_PORT",
    "FORGE_WORKSPACE_SERVER_URL",
    "AWS_REGION",
]

# SWE-bench specific AGENTS.md content written to /app/AGENTS.md inside the
# container so that Forge picks it up as project guidelines.
SWEBENCH_AGENTS_MD = """\
# SWE-bench Pro Agent Guidelines

You are solving a software engineering issue in this repository.

## Key Rules
1. The codebase is at /app - work only within this directory.
2. Read the issue carefully and understand what needs to change.
3. Make minimal, focused changes to fix the issue.
4. Do NOT modify or create test files.
5. Do NOT run the test suite - focus only on the implementation fix.
6. Make sure your changes compile/parse correctly.
7. If the issue mentions specific files, start by reading those files.
8. Use `git diff` to verify your changes before finishing.
"""

# Container setup script (mirrors bench/install-forge.sh.j2:1-26).
# Uses uv instead of pip because some SWE-bench tasks break pip.
CONTAINER_SETUP_SCRIPT = """\
#!/bin/bash
set -e

apt-get update -qq 2>/dev/null || true
apt-get install -y -qq curl git coreutils 2>/dev/null || true

# Install uv (pip may be broken by some tasks)
curl -LsSf https://astral.sh/uv/install.sh | env UV_INSTALL_DIR="/uv" sh
source /uv/env
export UV_PYTHON_INSTALL_DIR=/uv/python
export UV_CACHE_DIR=/uv/cache

# Create isolated Python environment for Forge dependencies
uv venv /uv/forge --python 3.13
source /uv/forge/bin/activate
uv pip install openai pydantic

# Symlink Python for non-login shells
ln -sf /uv/forge/bin/python /usr/local/bin/python
ln -sf /uv/forge/bin/python /usr/local/bin/python3

# Ensure forge binary is executable
chmod +x /usr/local/bin/forge

# Create forge config directory
mkdir -p /root/forge

# Verify
/usr/local/bin/forge --version 2>/dev/null || echo "Forge installed"
"""


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def resolve_forge_binary(cli_path: str | None) -> Path:
    """Resolve the forge binary path using precedence rules.

    1. --forge-bin CLI argument
    2. $FORGE_BIN environment variable
    3. ./target/x86_64-unknown-linux-musl/release/forge (MUSL, preferred)
    4. ./target/release/forge (fallback)
    """
    candidates: list[Path] = []

    if cli_path:
        candidates.append(Path(cli_path))
    if "FORGE_BIN" in os.environ:
        candidates.append(Path(os.environ["FORGE_BIN"]))

    candidates.extend(
        [
            Path("target/x86_64-unknown-linux-musl/release/forge"),
            Path("target/release/forge"),
        ]
    )

    for candidate in candidates:
        resolved = candidate.resolve()
        if resolved.exists() and os.access(resolved, os.X_OK):
            return resolved

    tried = "\n  ".join(str(c) for c in candidates)
    raise FileNotFoundError(
        f"Could not find forge binary. Tried:\n  {tried}\n\n"
        f"Build with: cross build --release --target x86_64-unknown-linux-musl\n"
        f"Or set $FORGE_BIN to the binary path."
    )


def create_problem_statement(row: dict[str, Any]) -> str:
    """Format the problem statement from a SWE-bench Pro dataset row.

    Matches helper_code/create_problem_statement.py from SWE-bench_Pro-os.
    """
    problem_statement = row.get("problem_statement", "")
    requirement = row.get("requirements", "")
    interface = row.get("interface", "")

    return f"""{problem_statement}

Requirements:
{requirement}

New interfaces introduced:
{interface}"""


def get_forge_env(
    model: str | None, provider: str | None
) -> dict[str, str]:
    """Build environment variables for Forge execution inside the container.

    Mirrors bench/forge_agent.py:657-729.
    """
    env: dict[str, str] = {}

    # Provider and model configuration
    if provider:
        env["FORGE_OVERRIDE_PROVIDER"] = provider
    elif "FORGE_OVERRIDE_PROVIDER" in os.environ:
        env["FORGE_OVERRIDE_PROVIDER"] = os.environ["FORGE_OVERRIDE_PROVIDER"]
    elif "DEFAULT_PROVIDER" in os.environ:
        env["FORGE_OVERRIDE_PROVIDER"] = os.environ["DEFAULT_PROVIDER"]

    if model:
        env["FORGE_OVERRIDE_MODEL"] = model
    elif "FORGE_OVERRIDE_MODEL" in os.environ:
        env["FORGE_OVERRIDE_MODEL"] = os.environ["FORGE_OVERRIDE_MODEL"]
    elif "DEFAULT_MODEL" in os.environ:
        env["FORGE_OVERRIDE_MODEL"] = os.environ["DEFAULT_MODEL"]

    # Non-interactive autonomous mode
    env["FORGE_BACKGROUND"] = "true"
    # 30-minute tool timeout for long-running tasks
    env["FORGE_TOOL_TIMEOUT"] = "1800"

    # Pass through API keys
    for key_var in API_KEY_VARS + URL_VARS:
        if key_var in os.environ:
            env[key_var] = os.environ[key_var]

    return env


def get_dockerhub_image(
    instance_id: str,
    dockerhub_tag: str,
    dockerhub_username: str,
) -> str:
    """Build the full Docker Hub image URI for a SWE-bench Pro instance."""
    return f"{dockerhub_username}/sweap-images:{dockerhub_tag}"


def strip_binary_hunks(patch: str) -> str:
    """Remove binary diff sections from a git patch.

    Matches swe_bench_pro_eval.py:strip_binary_hunks.
    """
    if not patch:
        return patch

    sections = re.split(r"(?=^diff --git )", patch, flags=re.MULTILINE)
    kept: list[str] = []
    for section in sections:
        if not section.strip():
            continue
        if re.search(r"^Binary files .* differ$", section, re.MULTILINE):
            continue
        if re.search(r"^GIT binary patch$", section, re.MULTILINE):
            continue
        kept.append(section)
    return "".join(kept)


def copy_host_file_to_container(
    container: Any, host_path: Path, container_path: str
) -> bool:
    """Copy a file from the host into a running Docker container."""
    import io
    import tarfile

    if not host_path.exists():
        return False

    data = host_path.read_bytes()
    # Build a tar archive in memory
    tar_stream = io.BytesIO()
    with tarfile.open(fileobj=tar_stream, mode="w") as tar:
        info = tarfile.TarInfo(name=os.path.basename(container_path))
        info.size = len(data)
        tar.addfile(info, io.BytesIO(data))
    tar_stream.seek(0)

    container.put_archive(os.path.dirname(container_path), tar_stream)
    return True


def copy_forge_binary_to_container(container: Any, forge_bin: Path) -> None:
    """Copy the forge binary into the container at /usr/local/bin/forge."""
    import io
    import tarfile

    data = forge_bin.read_bytes()
    tar_stream = io.BytesIO()
    with tarfile.open(fileobj=tar_stream, mode="w") as tar:
        info = tarfile.TarInfo(name="forge")
        info.size = len(data)
        info.mode = 0o755
        tar.addfile(info, io.BytesIO(data))
    tar_stream.seek(0)
    container.put_archive("/usr/local/bin", tar_stream)


def write_string_to_container(
    container: Any, content: str, container_path: str
) -> None:
    """Write a string as a file inside the container."""
    import io
    import tarfile

    data = content.encode("utf-8")
    tar_stream = io.BytesIO()
    with tarfile.open(fileobj=tar_stream, mode="w") as tar:
        info = tarfile.TarInfo(name=os.path.basename(container_path))
        info.size = len(data)
        info.mode = 0o644
        tar.addfile(info, io.BytesIO(data))
    tar_stream.seek(0)
    container.put_archive(os.path.dirname(container_path), tar_stream)


def exec_in_container(
    container: Any,
    command: str,
    workdir: str | None = None,
    environment: dict[str, str] | None = None,
    timeout: int | None = None,
) -> tuple[int, str]:
    """Execute a command inside a container and return (exit_code, output)."""
    kwargs: dict[str, Any] = {"cmd": ["bash", "-c", command]}
    if workdir:
        kwargs["workdir"] = workdir
    if environment:
        kwargs["environment"] = environment

    exec_id = container.client.api.exec_create(container.id, **kwargs)["Id"]
    output = container.client.api.exec_start(exec_id, stream=True)

    chunks: list[str] = []
    for chunk in output:
        if isinstance(chunk, bytes):
            chunks.append(chunk.decode("utf-8", errors="replace"))
        else:
            chunks.append(str(chunk))

    result = container.client.api.exec_inspect(exec_id)
    exit_code = result.get("ExitCode", -1)
    return exit_code, "".join(chunks)


# ---------------------------------------------------------------------------
# Core: run one instance
# ---------------------------------------------------------------------------


def run_instance(
    row: dict[str, Any],
    forge_bin: Path,
    output_dir: Path,
    model: str | None,
    provider: str | None,
    prefix: str,
    dockerhub_username: str,
    timeout: int,
    redo: bool,
    docker_platform: str | None,
) -> dict[str, Any]:
    """Run Forge against a single SWE-bench Pro instance inside Docker.

    Returns a result dict with instance_id, patch, success, and error info.
    """
    instance_id = row["instance_id"]
    dockerhub_tag = row["dockerhub_tag"]
    instance_dir = output_dir / instance_id
    instance_dir.mkdir(parents=True, exist_ok=True)

    result: dict[str, Any] = {
        "instance_id": instance_id,
        "patch": "",
        "prefix": prefix,
        "success": False,
        "error": None,
    }

    # --- Resumable: skip if patch already exists ---
    pred_file = instance_dir / f"{prefix}.pred"
    if not redo and pred_file.exists():
        content = pred_file.read_text()
        result["patch"] = content
        result["success"] = True
        result["skipped"] = True
        return result

    image_uri = get_dockerhub_image(instance_id, dockerhub_tag, dockerhub_username)
    conversation_id = str(uuid.uuid4())
    container = None
    client = docker.from_env()

    try:
        # --- Pull image ---
        print(f"[{instance_id}] Pulling image: {image_uri}")
        try:
            pull_kwargs: dict[str, Any] = {}
            if docker_platform:
                pull_kwargs["platform"] = docker_platform
            client.images.pull(image_uri, **pull_kwargs)
        except Exception as pull_err:
            # Fall back to local image if present
            try:
                client.images.get(image_uri)
                print(f"[{instance_id}] Using locally available image")
            except Exception:
                raise RuntimeError(
                    f"Failed to pull or find image: {image_uri}"
                ) from pull_err

        # --- Create container ---
        print(f"[{instance_id}] Creating container")
        run_kwargs: dict[str, Any] = {
            "detach": True,
            "entrypoint": "/bin/bash",
            "command": ["-c", "sleep infinity"],
            "stdin_open": True,
            "tty": False,
        }
        if docker_platform:
            run_kwargs["platform"] = docker_platform

        container = client.containers.run(image_uri, **run_kwargs)

        # --- Copy forge binary ---
        print(f"[{instance_id}] Copying forge binary")
        copy_forge_binary_to_container(container, forge_bin)

        # --- Copy forge config files from host ---
        forge_host_config = Path.home() / "forge"
        exec_in_container(container, f"mkdir -p {CONTAINER_FORGE_CONFIG_DIR}")
        for config_name in [".credentials.json", ".config.json"]:
            config_file = forge_host_config / config_name
            if config_file.exists():
                copy_host_file_to_container(
                    container,
                    config_file,
                    f"{CONTAINER_FORGE_CONFIG_DIR}/{config_name}",
                )

        # --- Run setup script ---
        print(f"[{instance_id}] Running setup script")
        write_string_to_container(
            container, CONTAINER_SETUP_SCRIPT, "/tmp/setup.sh"
        )
        exit_code, setup_output = exec_in_container(
            container, "bash /tmp/setup.sh"
        )
        if exit_code != 0:
            # Save setup log but continue - some failures are benign
            (instance_dir / "setup.log").write_text(setup_output)
            print(f"[{instance_id}] Setup script returned {exit_code} (continuing)")

        # --- Write AGENTS.md ---
        write_string_to_container(
            container, SWEBENCH_AGENTS_MD, f"{CONTAINER_APP_DIR}/AGENTS.md"
        )

        # --- Build and run Forge command ---
        prompt_text = create_problem_statement(row)
        escaped_prompt = shlex.quote(prompt_text)
        forge_env = get_forge_env(model, provider)

        forge_cmd = (
            f"{CONTAINER_FORGE_BIN} --verbose "
            f"--conversation-id {conversation_id} "
            f"-p {escaped_prompt}"
        )

        print(f"[{instance_id}] Running Forge (conversation: {conversation_id})")
        exit_code, forge_output = exec_in_container(
            container,
            forge_cmd,
            workdir=CONTAINER_APP_DIR,
            environment=forge_env,
        )

        # Save forge output log
        (instance_dir / "forge-output.log").write_text(forge_output)
        print(f"[{instance_id}] Forge exited with code {exit_code}")

        # --- Extract conversation dump (best-effort) ---
        try:
            dump_cmd = (
                f"{CONTAINER_FORGE_BIN} conversation dump {conversation_id}"
            )
            dump_exit, dump_output = exec_in_container(
                container, dump_cmd, workdir=CONTAINER_APP_DIR, environment=forge_env
            )
            if dump_exit == 0:
                # The dump command creates a file; try to cat it
                _, dump_json = exec_in_container(
                    container, "cat *-dump.json 2>/dev/null || true",
                    workdir=CONTAINER_APP_DIR,
                )
                if dump_json.strip():
                    (instance_dir / "conversation.json").write_text(dump_json)
        except Exception as dump_err:
            print(f"[{instance_id}] Conversation dump failed: {dump_err}")

        # --- Extract patch ---
        print(f"[{instance_id}] Extracting patch")
        # Stage all changes and generate diff
        _, _ = exec_in_container(
            container, "git add -A", workdir=CONTAINER_APP_DIR
        )
        _, patch_output = exec_in_container(
            container, "git diff --cached", workdir=CONTAINER_APP_DIR
        )

        # Fallback: try git diff HEAD if cached is empty
        if not patch_output.strip():
            _, patch_output = exec_in_container(
                container, "git diff HEAD", workdir=CONTAINER_APP_DIR
            )

        # Strip binary hunks
        patch_output = strip_binary_hunks(patch_output)

        # Save patch
        pred_file.write_text(patch_output)
        (instance_dir / f"{prefix}_patch.diff").write_text(patch_output)

        result["patch"] = patch_output
        result["success"] = bool(patch_output.strip())
        if not patch_output.strip():
            result["error"] = "Empty patch generated"
            print(f"[{instance_id}] Warning: empty patch")

    except Exception as e:
        error_msg = f"{type(e).__name__}: {e}"
        result["error"] = error_msg
        print(f"[{instance_id}] Error: {error_msg}")
        (instance_dir / "error.log").write_text(error_msg)

    finally:
        # --- Container cleanup ---
        if container:
            try:
                container.remove(force=True)
            except Exception:
                pass

    return result


# ---------------------------------------------------------------------------
# Dataset loading & filtering
# ---------------------------------------------------------------------------


def load_instances(
    instance_filter: str | None,
    instance_ids: str | None,
    slice_spec: str | None,
) -> list[dict[str, Any]]:
    """Load and filter SWE-bench Pro instances from HuggingFace."""
    print(f"Loading dataset: {DATASET_NAME} (split={DATASET_SPLIT})")
    dataset = load_dataset(DATASET_NAME, split=DATASET_SPLIT)

    instances = [dict(row) for row in dataset]
    print(f"Loaded {len(instances)} total instances")

    # Filter by specific instance IDs
    if instance_ids:
        id_set = {iid.strip() for iid in instance_ids.split(",")}
        instances = [r for r in instances if r["instance_id"] in id_set]
        print(f"Filtered to {len(instances)} instances by ID list")

    # Filter by regex
    if instance_filter and instance_filter != ".*":
        pattern = re.compile(instance_filter)
        instances = [r for r in instances if pattern.search(r["instance_id"])]
        print(f"Filtered to {len(instances)} instances by regex: {instance_filter}")

    # Apply slice
    if slice_spec:
        parts = slice_spec.split(":")
        if len(parts) == 2:
            start = int(parts[0]) if parts[0] else None
            stop = int(parts[1]) if parts[1] else None
            instances = instances[start:stop]
        elif len(parts) == 3:
            start = int(parts[0]) if parts[0] else None
            stop = int(parts[1]) if parts[1] else None
            step = int(parts[2]) if parts[2] else None
            instances = instances[start:stop:step]
        print(f"Sliced to {len(instances)} instances: [{slice_spec}]")

    return instances


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def parse_args() -> argparse.Namespace:
    """Parse command-line arguments."""
    parser = argparse.ArgumentParser(
        description="Run Forge agent on SWE-bench Pro instances",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Run on first 5 instances
  python swebench/run_forge.py \\
      --output-dir results/forge-sonnet \\
      --model anthropic/claude-sonnet-4 \\
      --slice 0:5

  # Run specific instances
  python swebench/run_forge.py \\
      --output-dir results/forge-test \\
      --instance-ids "instance_NodeBB__NodeBB-7b8bffd..."

  # Dry run (list instances without executing)
  python swebench/run_forge.py \\
      --output-dir results/test \\
      --slice 0:3 \\
      --dry-run
""",
    )

    # Required
    parser.add_argument(
        "--output-dir",
        required=True,
        help="Output directory for patches and logs",
    )

    # Model configuration
    parser.add_argument(
        "--model",
        default=None,
        help="Model name (e.g., anthropic/claude-sonnet-4). "
        "Default: from FORGE_OVERRIDE_MODEL env var or forge config",
    )
    parser.add_argument(
        "--provider",
        default=None,
        help="Provider name. Default: from FORGE_OVERRIDE_PROVIDER env var",
    )

    # Instance selection
    parser.add_argument(
        "--instance-filter",
        default=None,
        help="Filter instance IDs by regex (e.g., 'NodeBB')",
    )
    parser.add_argument(
        "--slice",
        default=None,
        help='Python slice notation (e.g., "0:10", "::2")',
    )
    parser.add_argument(
        "--instance-ids",
        default=None,
        help="Comma-separated list of specific instance IDs",
    )

    # Execution
    parser.add_argument(
        "--forge-bin",
        default=None,
        help="Path to forge binary. Default: $FORGE_BIN or target/*/release/forge",
    )
    parser.add_argument(
        "--num-workers",
        type=int,
        default=DEFAULT_NUM_WORKERS,
        help=f"Number of parallel workers (default: {DEFAULT_NUM_WORKERS})",
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=DEFAULT_TIMEOUT,
        help=f"Per-instance timeout in seconds (default: {DEFAULT_TIMEOUT})",
    )
    parser.add_argument(
        "--redo",
        action="store_true",
        help="Re-run even if output already exists",
    )
    parser.add_argument(
        "--dockerhub-username",
        default=DEFAULT_DOCKERHUB_USERNAME,
        help=f"DockerHub username for sweap-images (default: {DEFAULT_DOCKERHUB_USERNAME})",
    )
    parser.add_argument(
        "--docker-platform",
        default=None,
        help="Docker platform override (e.g., linux/amd64). Auto-detected on ARM",
    )
    parser.add_argument(
        "--prefix",
        default=DEFAULT_PREFIX,
        help=f"Prefix for patch output files (default: {DEFAULT_PREFIX})",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="List instances without running Forge",
    )

    return parser.parse_args()


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main() -> None:
    args = parse_args()
    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    # Resolve forge binary
    if not args.dry_run:
        forge_bin = resolve_forge_binary(args.forge_bin)
        print(f"Using forge binary: {forge_bin}")
    else:
        forge_bin = Path("/dev/null")  # placeholder for dry run

    # Auto-detect Docker platform on ARM hosts
    docker_platform = args.docker_platform
    if docker_platform is None:
        try:
            if py_platform.machine().lower() in {"arm64", "aarch64"}:
                docker_platform = "linux/amd64"
                print(f"Auto-detected ARM host, using platform: {docker_platform}")
        except Exception:
            pass

    # Load and filter instances
    instances = load_instances(
        instance_filter=args.instance_filter,
        instance_ids=args.instance_ids,
        slice_spec=args.slice,
    )

    if not instances:
        print("No instances to process. Check your filters.")
        return

    # Dry run: just list instances
    if args.dry_run:
        print(f"\n--- Dry Run: {len(instances)} instances ---")
        for row in instances:
            image = get_dockerhub_image(
                row["instance_id"], row["dockerhub_tag"], args.dockerhub_username
            )
            print(f"  {row['instance_id']} -> {image}")
        return

    print(f"\nProcessing {len(instances)} instances with {args.num_workers} workers")
    if args.model:
        print(f"Model: {args.model}")
    if args.provider:
        print(f"Provider: {args.provider}")

    # Run instances in parallel
    results: list[dict[str, Any]] = []
    successes = 0
    failures = 0
    skipped = 0

    with concurrent.futures.ThreadPoolExecutor(
        max_workers=args.num_workers
    ) as executor:
        future_to_instance = {
            executor.submit(
                run_instance,
                row=row,
                forge_bin=forge_bin,
                output_dir=output_dir,
                model=args.model,
                provider=args.provider,
                prefix=args.prefix,
                dockerhub_username=args.dockerhub_username,
                timeout=args.timeout,
                redo=args.redo,
                docker_platform=docker_platform,
            ): row
            for row in instances
        }

        pbar = tqdm(
            concurrent.futures.as_completed(future_to_instance),
            total=len(instances),
            desc="Running Forge",
        )

        for future in pbar:
            row = future_to_instance[future]
            try:
                result = future.result()
                results.append(result)
                if result.get("skipped"):
                    skipped += 1
                elif result["success"]:
                    successes += 1
                else:
                    failures += 1
            except Exception as exc:
                instance_id = row["instance_id"]
                print(f"[{instance_id}] Exception: {exc}")
                results.append(
                    {
                        "instance_id": instance_id,
                        "patch": "",
                        "prefix": args.prefix,
                        "success": False,
                        "error": str(exc),
                    }
                )
                failures += 1

            pbar.set_description(
                f"OK={successes} FAIL={failures} SKIP={skipped}"
            )

    # --- Generate patches.json ---
    patches = [
        {
            "instance_id": r["instance_id"],
            "patch": r.get("patch", ""),
            "prefix": r.get("prefix", args.prefix),
        }
        for r in results
        if r.get("patch", "").strip()
    ]

    patches_path = output_dir / "patches.json"
    with open(patches_path, "w") as f:
        json.dump(patches, f, indent=2)
    print(f"\nWrote {len(patches)} patches to {patches_path}")

    # --- Generate run_summary.json ---
    summary = {
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "total_instances": len(instances),
        "successful": successes,
        "failed": failures,
        "skipped": skipped,
        "patches_generated": len(patches),
        "model": args.model,
        "provider": args.provider,
        "prefix": args.prefix,
        "output_dir": str(output_dir),
    }

    summary_path = output_dir / "run_summary.json"
    with open(summary_path, "w") as f:
        json.dump(summary, f, indent=2)
    print(f"Wrote run summary to {summary_path}")

    # Print final summary
    print(f"\n{'=' * 60}")
    print(f"  SWE-bench Pro Forge Run Summary")
    print(f"{'=' * 60}")
    print(f"  Total instances:   {len(instances)}")
    print(f"  Successful:        {successes}")
    print(f"  Failed:            {failures}")
    print(f"  Skipped (cached):  {skipped}")
    print(f"  Patches generated: {len(patches)}")
    print(f"  Output dir:        {output_dir}")
    print(f"  Patches file:      {patches_path}")
    print(f"{'=' * 60}")


if __name__ == "__main__":
    main()
