#!/usr/bin/env python3
"""Extract and test Cavvy code blocks from Markdown documentation."""

from __future__ import annotations

import argparse
import os
import re
import shlex
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path


LANGUAGES = {"cay", "cavvy", "eol"}


@dataclass
class Block:
    path: Path
    line: int
    info: str
    code: str
    mode: str
    features: list[str]
    ignored: bool


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def exe_name(name: str) -> str:
    return f"{name}.exe" if os.name == "nt" else name


def parse_info(info: str) -> tuple[bool, str, list[str], bool]:
    normalized = info.replace(",", " ")
    try:
        parts = shlex.split(normalized)
    except ValueError:
        parts = normalized.split()

    if not parts:
        return False, "check", [], False

    language = parts[0].lower()
    if language not in LANGUAGES:
        return False, "check", [], False

    flags: set[str] = set()
    options: dict[str, str] = {}
    for part in parts[1:]:
        if "=" in part:
            key, value = part.split("=", 1)
            options[key.strip().lower()] = value.strip()
        else:
            flags.add(part.strip().lower())

    ignored = bool(flags & {"ignore", "ignored", "no-test", "notest", "skip"})
    if "run" in flags:
        mode = "run"
    elif "compile" in flags or "build" in flags:
        mode = "compile"
    else:
        mode = "check"

    features: list[str] = []
    if "feature" in options:
        features.extend(split_csv(options["feature"]))
    if "features" in options:
        features.extend(split_csv(options["features"]))
    for flag in flags:
        if flag.startswith("feature:"):
            features.append(flag.split(":", 1)[1])

    return True, mode, sorted(set(filter(None, features))), ignored


def split_csv(value: str) -> list[str]:
    return [item.strip() for item in value.split(";") for item in item.split(",")]


def discover_markdown(root: Path) -> list[Path]:
    paths: list[Path] = []
    readme = root / "README.md"
    if readme.exists():
        paths.append(readme)
    docs_dir = root / "docs"
    excluded_dir = docs_dir / "ESSO"
    if docs_dir.exists():
        paths.extend(
            sorted(
                path
                for path in docs_dir.rglob("*.md")
                if not path.is_relative_to(excluded_dir)
            )
        )
    return paths


def extract_blocks(path: Path) -> list[Block]:
    blocks: list[Block] = []
    in_block = False
    start_line = 0
    info = ""
    lines: list[str] = []

    for index, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not in_block:
            match = re.match(r"^```([^\n`]*)", line)
            if match:
                info = match.group(1).strip()
                in_block = True
                start_line = index
                lines = []
            continue

        if line.startswith("```"):
            is_cavvy, mode, features, ignored = parse_info(info)
            if is_cavvy:
                blocks.append(
                    Block(
                        path=path,
                        line=start_line,
                        info=info,
                        code="\n".join(lines).strip() + "\n",
                        mode=mode,
                        features=features,
                        ignored=ignored,
                    )
                )
            in_block = False
            continue

        lines.append(line)

    return blocks


def run_command(args: list[str], cwd: Path, timeout: int) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args,
        cwd=str(cwd),
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
    )


def ensure_release_tools(root: Path) -> None:
    missing = [
        root / "target" / "release" / exe_name("cay-check"),
        root / "target" / "release" / exe_name("cayc"),
    ]
    missing = [path for path in missing if not path.exists()]
    if missing:
        names = ", ".join(str(path.relative_to(root)) for path in missing)
        raise RuntimeError(
            f"missing release compiler tools: {names}. Run `cargo build --release` first."
        )


def build_release(root: Path) -> None:
    print("building release compiler...")
    result = run_command(["cargo", "build", "--release"], root, timeout=900)
    if result.returncode != 0:
        print(result.stdout)
        print(result.stderr, file=sys.stderr)
        raise RuntimeError("cargo build --release failed")


def block_slug(block: Block, index: int) -> str:
    rel = block.path.as_posix().replace("/", "_").replace("\\", "_")
    rel = re.sub(r"[^A-Za-z0-9_.-]+", "_", rel)
    return f"{index:03d}_{rel}_{block.line}"


def test_block(root: Path, temp_dir: Path, block: Block, index: int, keep_temp: bool) -> None:
    slug = block_slug(block, index)
    source = temp_dir / f"{slug}.cay"
    output = temp_dir / exe_name(slug)
    source.write_text(block.code, encoding="utf-8")

    cay_check = root / "target" / "release" / exe_name("cay-check")
    cayc = root / "target" / "release" / exe_name("cayc")

    if block.mode == "check" and not block.features:
        cmd = [str(cay_check), str(source)]
        result = run_command(cmd, root, timeout=120)
    else:
        cmd = [str(cayc), str(source), str(output)]
        for feature in block.features:
            cmd.append(f"--feature={feature}")
        result = run_command(cmd, root, timeout=180)
        if result.returncode == 0 and block.mode == "run":
            result = run_command([str(output)], root, timeout=30)
            cmd = [str(output)]

    if result.returncode != 0:
        rel = block.path.relative_to(root)
        raise RuntimeError(
            "\n".join(
                [
                    f"{rel}:{block.line}: doc example failed",
                    f"info: ```{block.info}",
                    f"command: {' '.join(cmd)}",
                    "--- stdout ---",
                    result.stdout.strip(),
                    "--- stderr ---",
                    result.stderr.strip(),
                ]
            )
        )

    if not keep_temp:
        for path in [source, output, output.with_suffix(".ll")]:
            try:
                path.unlink()
            except FileNotFoundError:
                pass


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build", action="store_true", help="run cargo build --release first")
    parser.add_argument("--keep-temp", action="store_true", help="keep generated doc test files")
    parser.add_argument("--root", type=Path, default=repo_root(), help="repository root")
    args = parser.parse_args()

    root = args.root.resolve()
    if args.build:
        build_release(root)

    ensure_release_tools(root)

    blocks: list[Block] = []
    for path in discover_markdown(root):
        blocks.extend(extract_blocks(path))

    tested = [block for block in blocks if not block.ignored]
    skipped = len(blocks) - len(tested)

    with tempfile.TemporaryDirectory(prefix="cavvy_doc_tests_", delete=not args.keep_temp) as temp_dir_name:
        temp_dir = Path(temp_dir_name)
        start = time.time()
        for index, block in enumerate(tested, start=1):
            rel = block.path.relative_to(root)
            feature_text = f" features={','.join(block.features)}" if block.features else ""
            print(f"[{index}/{len(tested)}] {rel}:{block.line} {block.mode}{feature_text}")
            test_block(root, temp_dir, block, index, args.keep_temp)

        elapsed = time.time() - start
        print(f"doc tests passed: {len(tested)} tested, {skipped} skipped, {elapsed:.1f}s")

    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"doc tests failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
