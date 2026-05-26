"""
Fuzz Test Generator - Main Orchestrator
调用 gen/ 下所有集群生成器，生成 fuzz/*.cay + runner.cay + config.json
"""
import sys
import os
import importlib
import json
from gen_utils import CLUSTER_DEFS, setup_cluster_dir, write_runner, write_config

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))


def get_cluster_module(cluster_id):
    cluster_name = CLUSTER_DEFS[cluster_id]
    mod_name = f"gen.cluster_{cluster_id}_{cluster_name}"
    return importlib.import_module(mod_name)


def run_cluster(cluster_id):
    cluster_name = CLUSTER_DEFS[cluster_id]
    print(f"\n{'='*60}")
    print(f"  Generating cluster {cluster_id}: {cluster_name}")
    print(f"{'='*60}")

    cluster_dir, fuzz_dir = setup_cluster_dir(cluster_id)

    try:
        mod = get_cluster_module(cluster_id)
    except ImportError as e:
        print(f"  SKIP: module {cluster_id} not found: {e}")
        return {"cluster_id": cluster_id, "files": 0, "status": "SKIP"}

    if not hasattr(mod, "generate"):
        print(f"  SKIP: no generate() in {cluster_id}")
        return {"cluster_id": cluster_id, "files": 0, "status": "SKIP"}

    fuzz_files, config_data = mod.generate(cluster_id, cluster_name, cluster_dir, fuzz_dir)

    write_runner(cluster_dir, cluster_id, cluster_name, fuzz_files)
    write_config(cluster_dir, cluster_id, cluster_name, config_data)

    print(f"  Generated {len(fuzz_files)} fuzz files + runner.cay + config.json")
    return {"cluster_id": cluster_id, "files": len(fuzz_files), "status": "OK"}


def main():
    results = []
    total_files = 0

    for cid in sorted(CLUSTER_DEFS.keys()):
        r = run_cluster(cid)
        results.append(r)
        total_files += r["files"]

    print(f"\n{'='*60}")
    print(f"  TOTAL: {total_files} fuzz files across {len([r for r in results if r['status']=='OK'])} clusters")
    print(f"{'='*60}")

    ok = [r for r in results if r["status"] == "OK"]
    skip = [r for r in results if r["status"] == "SKIP"]
    if skip:
        print(f"  SKIPPED: {len(skip)} clusters")
        for s in skip:
            print(f"    - {CLUSTER_DEFS[s['cluster_id']]}")


if __name__ == "__main__":
    main()
