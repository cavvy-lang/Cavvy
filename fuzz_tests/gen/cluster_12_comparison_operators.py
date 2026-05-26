"""
Cluster 12 - comparison_operators: 比较运算符模糊测试
"""
import sys,os,random
sys.path.insert(0,os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

def generate(cluster_id,cluster_name,cluster_dir,fuzz_dir):
    reset_seed(); files=[]; cfg=[]
    OPS=["==","!=","<","<=",">",">="]
    for fi in range(100):
        reset_seed(); cn=unique_class_name(cluster_id,fi); b=FuzzClassBuilder(cn,cluster_name)
        for mi in range(random.randint(8,12)):
            mn=unique_method_name("test_cmp",mi)
            op=random.choice(OPS)
            a=random.randint(-100,100); bv=random.randint(-100,100)
            try: exp=eval(f"{a}{op}{bv}")
            except: exp=False
            b.add_method(mn,[f"int a = {a}; int b = {bv}; boolean c = a {op} b;",
                'print(\\\"cmp=\\\"); println(c);','println("OK");'])
        fn=f"fuzz_{fi:04d}.cay"; write_cay_file(fuzz_dir,fn,b.build())
        files.append({"filename":fn,"class":cn}); cfg.append({"file":fn,"class":cn})
    return files,cfg
