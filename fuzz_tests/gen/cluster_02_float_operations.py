"""
Cluster 02 - float_operations: 浮点运算模糊测试
"""
import sys, os, random
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

FLOATS = [-3.40282e38, -1.0e10, -1.0, -0.5, -0.0, 0.0, 0.5, 1.0, 1.0e10, 3.40282e38,
          3.14159, 2.71828, -2.71828, 1.41421, 0.00001, -0.00001]
DOUBLES = FLOATS + [-1.79769e308, 1.79769e308, 1e-308, -1e-308]


def generate(cluster_id, cluster_name, cluster_dir, fuzz_dir):
    reset_seed()
    fuzz_files = []
    config_data = []
    for fi in range(120):
        reset_seed()
        cn = unique_class_name(cluster_id, fi)
        b = FuzzClassBuilder(cn, cluster_name)
        for mi in range(random.randint(8, 14)):
            use_double = random.choice([True, False])
            edges = DOUBLES if use_double else FLOATS
            tp = "double" if use_double else "float"
            op = random.choice(["+", "-", "*", "/"])
            a = random.choice(edges)
            bv = random.choice(edges)
            if op == "/" and bv == 0:
                bv = 1.0
            mn = unique_method_name("test_fp", mi)
            b.add_method(mn, [
                f"{tp} a = {a};",
                f"{tp} b = {bv};",
                f"{tp} c = a {op} b;",
                f'print("{tp} {op} result=");',
                "println(c);",
                'println("OK");'
            ])
        fn = f"fuzz_{fi:04d}.cay"
        write_cay_file(fuzz_dir, fn, b.build())
        fuzz_files.append({"filename": fn, "class": cn})
        config_data.append({"file": fn, "class": cn})
    return fuzz_files, config_data
