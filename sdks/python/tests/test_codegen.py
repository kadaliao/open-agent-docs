"""生成的代码是最新的（openapi.json / operations.json 改了要重新跑生成器）。"""
import subprocess
import sys
from pathlib import Path

GEN = Path(__file__).resolve().parents[1] / "codegen" / "generate.py"


def test_generated_code_is_up_to_date() -> None:
    r = subprocess.run([sys.executable, str(GEN), "--check"], capture_output=True, text=True)
    assert r.returncode == 0, r.stderr + r.stdout
