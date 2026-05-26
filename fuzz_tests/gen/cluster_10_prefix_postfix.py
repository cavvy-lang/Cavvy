"""
Cluster 10 - prefix_postfix: 自增自减模糊测试
"""
import sys,os,random
sys.path.insert(0,os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

def generate(cluster_id,cluster_name,cluster_dir,fuzz_dir):
    reset_seed(); files=[]; cfg=[]
    for fi in range(100):
        reset_seed(); cn=unique_class_name(cluster_id,fi); b=FuzzClassBuilder(cn,cluster_name)
        for mi in range(random.randint(8,12)):
            mn=unique_method_name("test_pp",mi)
            init=random.randint(0,100); op=random.choice(["++","--"])
            if random.choice([True,False]):
                b.add_method(mn,[f"int x = {init};",f"int y = {op}x;",
                    "print(\\\"x=\\\"); println(x);","print(\\\"y=\\\"); println(y);",'println("OK");'])
            else:
                b.add_method(mn,[f"int x = {init};",f"int y = x{op};",
                    "print(\\\"x=\\\"); println(x);","print(\\\"y=\\\"); println(y);",'println("OK");'])
        fn=f"fuzz_{fi:04d}.cay"; write_cay_file(fuzz_dir,fn,b.build())
        files.append({"filename":fn,"class":cn}); cfg.append({"file":fn,"class":cn})
    return files,cfg
