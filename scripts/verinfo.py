#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Read build metadata from ``.verinfo`` for CI workflows.

The nightly workflow used to derive the version from git tags.  That breaks for
untagged commits and does not reflect the compiler build number.  This script is
the single parser used by both the nightly and push CI workflows.

Usage::

    python scripts/verinfo.py
    python scripts/verinfo.py --json
    python scripts/verinfo.py --github-output

``--github-output`` writes ``tag``, ``version``, ``build``, ``date``,
``basename`` and ``short_sha`` to ``$GITHUB_OUTPUT``.
"""

from __future__ import annotations

import argparse
import configparser
import datetime as _dt
import json
import os
import sys
from pathlib import Path

DEFAULT_SECTION = "CAYC"


def parse_verinfo(path: Path, section: str) -> dict[str, str]:
    config = configparser.ConfigParser()
    if not config.read(path, encoding="utf-8"):
        raise SystemExit(f"cannot read {path}")

    if section not in config:
        raise SystemExit(f"section [{section}] not found in {path}")

    raw = config[section]
    version = (raw.get("version") or "0.0.0").strip().strip('"').strip("'")
    build = (raw.get("build") or "0").strip().strip('"').strip("'") or "0"
    if version.startswith("v"):
        version = version[1:]

    date = os.environ.get("CAVVY_BUILD_DATE") or _dt.datetime.now(
        _dt.timezone.utc
    ).strftime("%Y%m%d")
    sha = os.environ.get("GITHUB_SHA", "").strip()
    short_sha = sha[:7] if sha else "local"

    tag = f"v{version}"
    basename = f"cavvy-{tag}-build{build}-{date}"
    return {
        "tag": tag,
        "version": version,
        "build": build,
        "date": date,
        "basename": basename,
        "short_sha": short_sha,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verinfo", default=".verinfo", type=Path)
    parser.add_argument("--section", default=DEFAULT_SECTION)
    parser.add_argument(
        "--json",
        action="store_true",
        help="print a JSON object instead of key=value lines",
    )
    parser.add_argument(
        "--github-output",
        action="store_true",
        help="append key=value lines to $GITHUB_OUTPUT",
    )
    args = parser.parse_args()

    values = parse_verinfo(args.verinfo, args.section)

    if args.json:
        print(json.dumps(values, ensure_ascii=False))
    else:
        for key, value in values.items():
            print(f"{key}={value}")

    if args.github_output:
        output = os.environ.get("GITHUB_OUTPUT")
        if not output:
            print("GITHUB_OUTPUT is not set", file=sys.stderr)
            return 1
        with open(output, "a", encoding="utf-8") as handle:
            for key, value in values.items():
                handle.write(f"{key}={value}\n")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
