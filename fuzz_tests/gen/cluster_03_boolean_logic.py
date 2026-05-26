"""
Cluster 03 - boolean_logic: 布尔逻辑模糊测试
"""
import sys, os, random
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *


def generate(cluster_id, cluster_name, cluster_dir, fuzz_dir):
    reset_seed()
    fuzz_files = []
    config_data = []
    EXPRS = [
        "boolean a = {t}; boolean b = {f}; boolean c = a && b;",
        "boolean a = {t}; boolean b = {f}; boolean c = a || b;",
        "boolean a = {t}; boolean c = !a;",
        "boolean a = {t}; boolean b = {f}; boolean c = a && b; boolean d = a || b;",
        "boolean a = {t}; boolean b = {f}; boolean c = !(a && b);",
        "boolean a = {t}; boolean c = a && a;",
        "boolean a = {t}; boolean b = {f}; boolean c = (a && b) || (!a && !b);",
        "boolean a = {t}; boolean b = {f}; boolean c = !a && !b;",
        "boolean a = {t}; boolean b = {t}; boolean c = a || b;",
        "boolean a = {f}; boolean b = {f}; boolean c = a && !b;",
    ]
    for fi in range(100):
        reset_seed()
        cn = unique_class_name(cluster_id, fi)
        b = FuzzClassBuilder(cn, cluster_name)
        for mi in range(random.randint(10, 15)):
            t = random.choice(["true", "false"])
            f = random.choice(["true", "false"])
            expr = random.choice(EXPRS).format(t=t, f=f)
            mn = unique_method_name("test_bool", mi)
            b.add_method(mn, [expr, 'println("OK");'])
        fn = f"fuzz_{fi:04d}.cay"
        write_cay_file(fuzz_dir, fn, b.build())
        fuzz_files.append({"filename": fn, "class": cn})
        config_data.append({"file": fn, "class": cn})
    return fuzz_files, config_data
