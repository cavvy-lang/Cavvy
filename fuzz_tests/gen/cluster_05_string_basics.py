"""
Cluster 05 - string_basics: 字符串基础操作模糊测试
"""
import sys, os, random
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

STRINGS = [
    '"hello"', '"world"', '"Cavvy"', '""', '"12345"', '"  spaces  "',
    '"newline\\nhere"', '"!@#$%^&*()"', '"The quick brown fox"',
    '"jumps over"', '"the lazy dog"', '"a"', '"ab"', '"abc"',
    '"abcdef"', '"zyxwvutsrqponmlkjihgfedcba"', '"   "',
    '"0xDEADBEEF"', '"3.141592653589793"', '"true"', '"false"', '"null"',
    '"String with \\\'quotes\\\' inside"',
    '"MixedCase123"', '"UPPERCASE"', '"lowercase"', '"CamelCaseStyle"',
    '"snake_case_style"', '"kebab-case-style"', '"中文测试"',
]


def generate(cluster_id, cluster_name, cluster_dir, fuzz_dir):
    reset_seed()
    fuzz_files = []
    config_data = []
    for fi in range(110):
        reset_seed()
        cn = unique_class_name(cluster_id, fi)
        b = FuzzClassBuilder(cn, cluster_name)
        for mi in range(random.randint(8, 14)):
            s1 = random.choice(STRINGS)
            s2 = random.choice(STRINGS)
            use_concat = random.choice([True, False])
            if use_concat:
                body = [
                    f"String s1 = {s1};",
                    f"String s2 = {s2};",
                    "String s3 = s1 + s2;",
                    "println(s3);",
                ]
            else:
                body = [
                    f"String s = {s1};",
                    "print(\\\"len=\\\");",
                    "println(s.length());",
                ]
            mn = unique_method_name("test_str", mi)
            b.add_method(mn, body + ['println("OK");'])
        fn = f"fuzz_{fi:04d}.cay"
        write_cay_file(fuzz_dir, fn, b.build())
        fuzz_files.append({"filename": fn, "class": cn})
        config_data.append({"file": fn, "class": cn})
    return fuzz_files, config_data
