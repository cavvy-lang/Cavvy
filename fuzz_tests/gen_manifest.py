"""
Generate assertion config for fuzz runner validation.
Reads config.json and produces assert patterns for Rust tests.
"""
import json, os, glob, sys

OUTPUT = os.path.join(os.path.dirname(__file__), "output")
PROJECT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

def main():
    summary = {}
    for runner_path in sorted(glob.glob(os.path.join(OUTPUT, "cluster_*", "runner.cay"))):
        cluster_dir = os.path.dirname(runner_path)
        cluster_name = os.path.basename(cluster_dir)
        config_path = os.path.join(cluster_dir, "config.json")
        if not os.path.exists(config_path):
            continue
        with open(config_path, "r", encoding="utf-8") as f:
            cfg = json.load(f)
        n = len(cfg.get("tests", []))
        summary[cluster_name] = n
        print(f"{cluster_name}: {n} fuzz files")

    total_files = sum(summary.values())
    print(f"\nTotal clusters: {len(summary)}")
    print(f"Total fuzz files: {total_files}")

    # export for Rust
    out_json = os.path.join(PROJECT, "fuzz_tests", "cluster_manifest.json")
    with open(out_json, "w", encoding="utf-8") as f:
        json.dump(summary, f, indent=2)
    print(f"Manifest written to {out_json}")


if __name__ == "__main__":
    main()
