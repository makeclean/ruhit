#!/usr/bin/env python3
"""Generate checked-in WASP oracle projections for tests/wasp/manifest.toml.

Requires a built tools/wasp_dump binary (see build.sh).
"""
from __future__ import annotations

import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "tests/wasp/manifest.toml"
DUMP = pathlib.Path(
    os.environ.get("WASP_DUMP", ROOT / "target/wasp_dump/bin/wasp_dump")
)
EXPECTED = ROOT / "tests/wasp/expected"


def run(case: dict) -> dict:
    with tempfile.TemporaryDirectory(prefix="ruhit-wasp-") as tmp:
        tmp_path = pathlib.Path(tmp)
        if case["suite"] == "hit":
            workdir = case.get("workdir")
            if workdir:
                src_dir = ROOT / "tests/wasp" / workdir
                for item in src_dir.iterdir():
                    if item.is_file():
                        shutil.copy2(item, tmp_path / item.name)
            input_path = ROOT / "tests/wasp" / case["input"]
            local_input = tmp_path / input_path.name
            if not local_input.exists():
                shutil.copy2(input_path, local_input)
            proc = subprocess.run(
                [str(DUMP), "hit", local_input.name, "--name", "stream input"],
                cwd=tmp_path,
                text=True,
                capture_output=True,
            )
            if not proc.stdout.strip():
                raise RuntimeError(f"dumper failed for {case['id']}: {proc.stderr}")
            return json.loads(proc.stdout)
        if case["suite"] == "hive":
            # All HIVE inputs are copied so !include fixtures resolve.
            for item in (ROOT / "tests/wasp/inputs/hive/inputs").iterdir():
                if item.is_file():
                    shutil.copy2(item, tmp_path / item.name)
            schema = ROOT / "tests/wasp" / case["schema"]
            fail_input = ROOT / "tests/wasp" / case["fail_input"]
            proc = subprocess.run(
                [str(DUMP), "hive", str(schema), fail_input.name],
                cwd=tmp_path,
                text=True,
                capture_output=True,
            )
            if not proc.stdout.strip():
                raise RuntimeError(f"dumper failed for {case['id']}: {proc.stderr}")
            return json.loads(proc.stdout)
        raise ValueError(case["suite"])


def main() -> int:
    if not DUMP.exists():
        print(f"missing dumper: {DUMP}", file=sys.stderr)
        return 1
    manifest = tomllib.loads(MANIFEST.read_text())
    for case in manifest["case"]:
        projection = run(case)
        out_dir = EXPECTED / case["suite"]
        out_dir.mkdir(parents=True, exist_ok=True)
        out = out_dir / f"{case['id'].split('/', 1)[1]}.json"
        out.write_text(json.dumps(projection, indent=2, ensure_ascii=False) + "\n")
        print(f"wrote {out.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
