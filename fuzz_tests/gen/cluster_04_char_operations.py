"""
Cluster 04 - char_operations: 字符操作模糊测试
"""
import sys, os, random
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

CHARS = list("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*()_+-=[]{}|;:,.<>/?")


def generate(cluster_id, cluster_name, cluster_dir, fuzz_dir):
    reset_seed()
    fuzz_files = []
    config_data = []
    for fi in range(100):
        reset_seed()
        cn = unique_class_name(cluster_id, fi)
        b = FuzzClassBuilder(cn, cluster_name)
        for mi in range(random.randint(8, 12)):
            c1 = random.choice(CHARS)
            c2 = random.choice(CHARS)
            mn = unique_method_name("test_char", mi)
            body = [
                f"char a = '{c1}';",
                f"char b = '{c2}';",
                "print(\\\"char test: \\\");",
                "print(a);",
                "print(\\\" vs \\\");",
                "println(b);",
                'println("OK");'
            ]
            if c1 == '"':
                body = [f"char a = 'A'; char b = 'Z';", 'println("char escape OK");', 'println("OK");']
            if c1 == '\\':
                body = [f"char a = 'A'; char b = 'Z';", 'println("char backslash OK");', 'println("OK");']
            b.add_method(mn, body)
        fn = f"fuzz_{fi:04d}.cay"
        write_cay_file(fuzz_dir, fn, b.build())
        fuzz_files.append({"filename": fn, "class": cn})
        config_data.append({"file": fn, "class": cn})
    return fuzz_files, config_data
