import sys
import importlib.util
from pathlib import Path

spec = importlib.util.spec_from_file_location("doc_test", Path(__file__).resolve().parent / "scripts" / "doc-test.py")
mod = importlib.util.module_from_spec(spec)
# avoid dataclass __module__ issue by setting __name__
spec.loader.exec_module(mod)

root = Path(__file__).resolve().parent
blocks = []
for p in mod.discover_markdown(root):
    blocks.extend(mod.extract_blocks(p))
tested = [b for b in blocks if not b.ignored]
for i in [14, 15, 16]:
    b = tested[i - 1]
    print(f"--- block {i} ---")
    print(f"path={b.path} line={b.line}")
    print(f"info={b.info!r}")
    print(f"code_len={len(b.code)}")
    print(f"code={b.code!r}")
    print()
