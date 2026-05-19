import subprocess
import os
import sys

PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# Find autoparq binary in the same venv as the running Python interpreter
AUTOPARQ = os.path.join(os.path.dirname(sys.executable), "autoparq")
FIXTURES = "tests/fixtures"


def test_tune_exit_1_improvement_available():
    """Tuning an unoptimized file returns exit code 1 (improvement available)."""
    result = subprocess.run(
        [
            AUTOPARQ,
            "tune",
            f"{FIXTURES}/low_cardinality_strings.parquet",
            "--engine",
            "unknown",
            "--priority",
            "balanced",
        ],
        capture_output=True,
        cwd=PROJECT_ROOT,
    )
    assert result.returncode == 1, (
        f"Expected 1, got {result.returncode}. stderr: {result.stderr.decode()}"
    )


def test_tune_exit_2_bad_file():
    """Tuning a nonexistent file returns exit code 2."""
    result = subprocess.run(
        [AUTOPARQ, "tune", "/nonexistent_file_xyz.parquet"],
        capture_output=True,
        cwd=PROJECT_ROOT,
    )
    assert result.returncode == 2


def test_info_exit_0():
    """Info on a valid file returns exit code 0."""
    result = subprocess.run(
        [AUTOPARQ, "info", f"{FIXTURES}/multi_column.parquet"],
        capture_output=True,
        cwd=PROJECT_ROOT,
    )
    assert result.returncode == 0, (
        f"Expected 0, got {result.returncode}. stderr: {result.stderr.decode()}"
    )


def test_info_exit_2_bad_file():
    """Info on a nonexistent file returns exit code 2."""
    result = subprocess.run(
        [AUTOPARQ, "info", "/nonexistent_file_xyz.parquet"],
        capture_output=True,
        cwd=PROJECT_ROOT,
    )
    assert result.returncode == 2


def test_tune_exit_0_no_improvement():
    """Tuning a file with --min-improvement 200 returns exit code 0 (threshold not met)."""
    # 200% threshold is impossible to meet, so exit code must be 0
    result = subprocess.run(
        [
            AUTOPARQ,
            "tune",
            f"{FIXTURES}/low_cardinality_strings.parquet",
            "--min-improvement",
            "200",
        ],
        capture_output=True,
        cwd=PROJECT_ROOT,
    )
    assert result.returncode == 0, (
        f"Expected 0, got {result.returncode}. stderr: {result.stderr.decode()}"
    )
