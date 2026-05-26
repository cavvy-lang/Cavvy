"""
Cluster 15 - ternary_operator: 三元运算符模糊测试
"""
import sys,os,random
sys.path.insert(0,os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

def generate(cluster_id,cluster_name,cluster_dir,fuzz_dir):
    reset_seed(); files=[]; cfg=[]
    for fi in range(100):
        reset_seed(); cn=unique_class_name(cluster_id,fi); b=FuzzClassBuilder(cn,cluster_name)
        for mi in range(random.randint(8,12)):
            mn=unique_method_name("test_tern",mi)
            a=random.randint(-100,100); bv=random.randint(-100,100)
            conds=[f"{a}>0",f"{a}<0",f"{a}==0",f"{a}>={bv}",f"{a}<=0",f"{a}!={bv}"]
            c=random.choice(conds)
            b.add_method(mn,[f"int a = {a}; int b = {bv};",
                f"int r = ({c}) ? a : b;",
                'print(\\\"tern=\\\"); println(r);','println("OK");'])
        fn=f"fuzz_{fi:04d}.cay"; write_cay_file(fuzz_dir,fn,b.build())
        files.append({"filename":fn,"class":cn}); cfg.append({"file":fn,"class":cn})
    return files,cfg
