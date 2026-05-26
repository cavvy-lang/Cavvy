"""
Cluster 01 - int_arithmetic: 整数算术运算模糊测试
覆盖: + - * / % 及边缘值、组合表达式
"""
import sys, os
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *
INT_EDGES = [-2147483648, -2147483647, -65536, -32769, -32768, -1, 0, 1, 2,
                 255, 256, 32767, 32768, 65535, 65536, 100000, 1000000, 2147483646, 2147483647]

def generate(cluster_id, cluster_name, cluster_dir, fuzz_dir):
    reset_seed()
    fuzz_files = []
    config_data = []

    
    OPS = [("add", "+"), ("sub", "-"), ("mul", "*"), ("div", "/"), ("mod", "%")]

    for fi in range(120):
        reset_seed()
        class_name = unique_class_name(cluster_id, fi)
        builder = FuzzClassBuilder(class_name, cluster_name)

        num_methods = random.randint(8, 14)
        for mi in range(num_methods):
            op_name, op_sym = random.choice(OPS)
            mname = unique_method_name(f"test_{op_name}", mi)
            lines = _gen_arith_body(op_name, op_sym, mi)
            builder.add_method(mname, lines)

        fname = f"fuzz_{fi:04d}.cay"
        write_cay_file(fuzz_dir, fname, builder.build())
        fuzz_files.append({"filename": fname, "class": class_name, "methods": range(num_methods)})
        config_data.append({
            "file": fname,
            "class": class_name,
            "method_count": num_methods,
            "ops_covered": "int arithmetic with edge values"
        })

    return fuzz_files, config_data


def _gen_arith_body(op_name, op_sym, seed):
    random.seed(20260526 + seed * 137)
    a = random.choice(INT_EDGES)
    b = random.choice(INT_EDGES)
    if op_name == "div" and b == 0:
        b = 1
    if op_name == "mod" and b == 0:
        b = 1
    c = _compute_int(a, b, op_name)
    return [
        f"int a = {a};",
        f"int b = {b};",
        f"int c = a {op_sym} b;",
        f'print("{a} {op_sym} {b} = ");',
        f"println(c);",
        f"if (c == {c}) {{ println(\"OK\"); }}",
    ]


def _compute_int(a, b, op):
    try:
        if op == "add": return a + b
        if op == "sub": return a - b
        if op == "mul": return a * b
        if op == "div": return a // b
        if op == "mod": return a % b
    except:
        return 0
    return 0
