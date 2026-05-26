"""
Cluster 41 - auto_inference: auto类型推断模糊测试
"""
import sys,os,random
sys.path.insert(0,os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

def generate(cluster_id,cluster_name,cluster_dir,fuzz_dir):
    reset_seed(); files=[]; cfg=[]
    for fi in range(100):
        reset_seed(); cn=unique_class_name(cluster_id,fi); b=FuzzClassBuilder(cn,cluster_name)
        for mi in range(random.randint(8,14)):
            mn=unique_method_name("test_auto",mi)
            tp=random.randint(0,4)
            if tp==0: body=['auto x = 42;','print(\\\"auto_int=\\\"); println(x);']
            elif tp==1: body=['auto y = 3.14;','print(\\\"auto_dbl=\\\"); println(y);']
            elif tp==2: body=['auto z = \\\"hello\\\";','print(\\\"auto_str=\\\"); println(z);']
            elif tp==3: body=['auto b = true;','print(\\\"auto_bool=\\\"); println(b);']
            else: body=['auto c = 100L;','print(\\\"auto_long=\\\"); println(c);']
            body+=['println("OK");']
            b.add_method(mn,body)
        fn=f"fuzz_{fi:04d}.cay"; write_cay_file(fuzz_dir,fn,b.build())
        files.append({"filename":fn,"class":cn}); cfg.append({"file":fn,"class":cn})
    return files,cfg
