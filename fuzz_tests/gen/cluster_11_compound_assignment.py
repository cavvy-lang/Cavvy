"""
Cluster 11 - compound_assignment: 复合赋值模糊测试
"""
import sys,os,random
sys.path.insert(0,os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

def generate(cluster_id,cluster_name,cluster_dir,fuzz_dir):
    reset_seed(); files=[]; cfg=[]
    OPS=[("+=","+"),("-=","-"),("*=","*"),("/=","/"),("%=","%")]
    for fi in range(100):
        reset_seed(); cn=unique_class_name(cluster_id,fi); b=FuzzClassBuilder(cn,cluster_name)
        for mi in range(random.randint(8,14)):
            mn=unique_method_name("test_ca",mi)
            osym,csym=random.choice(OPS)
            a=random.randint(1,100); bv=random.randint(1,10)
            try: exp=eval(f"{a}{csym}{bv}")
            except: exp=0
            b.add_method(mn,[f"int x = {a};",f"x {osym} {bv};",
                f"print(\\\"{a}{osym}{bv}=\\\"); println(x);",'println("OK");'])
        fn=f"fuzz_{fi:04d}.cay"; write_cay_file(fuzz_dir,fn,b.build())
        files.append({"filename":fn,"class":cn}); cfg.append({"file":fn,"class":cn})
    return files,cfg
