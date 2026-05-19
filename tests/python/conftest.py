import subprocess
import os
import sys

PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def pytest_configure(config):
    fixture_dir = os.path.join(PROJECT_ROOT, "tests", "fixtures")
    if not os.path.exists(os.path.join(fixture_dir, "multi_column.parquet")):
        env = os.environ.copy()
        env["PYO3_PYTHON"] = sys.executable
        subprocess.run(
            ["cargo", "run", "--example", "gen_fixtures"],
            cwd=PROJECT_ROOT,
            env=env,
            check=True,
        )
