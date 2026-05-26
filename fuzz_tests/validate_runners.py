"""Validate all fuzz runners with cay-check"""
import os, subprocess, sys, glob, json

PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CAY_CHECK = os.path.join(PROJECT_ROOT, "target", "release", "cay-check.exe")
OUTPUT_DIR = os.path.join(PROJECT_ROOT, "fuzz_tests", "output")

if not os.path.exists(CAY_CHECK):
    print("ERROR: cay-check.exe not found. Run cargo build --release first.")
    sys.exit(1)

runners = sorted(glob.glob(os.path.join(OUTPUT_DIR, "cluster_*", "runner.cay")))
total = len(runners)
ok = 0
fail = 0
results = {}

for i, r in enumerate(runners):
    cluster = os.path.basename(os.path.dirname(r))
    result = subprocess.run([CAY_CHECK, r], capture_output=True, text=True, timeout=30)
    status = "OK" if result.returncode == 0 else "FAIL"
    if status == "OK":
        ok += 1
    else:
        fail += 1
        results[cluster] = result.stderr[:300]
    print(f"[{i+1:03d}/{total}] {cluster}: {status}")

print(f"\nTotal: {total}, OK: {ok}, FAIL: {fail}")

if fail > 0:
    print("\nFailures:")
    for cluster, err in results.items():
        print(f"  {cluster}: {err[:200]}")
    sys.exit(1)
else:
    print("All runners pass cay-check!")
