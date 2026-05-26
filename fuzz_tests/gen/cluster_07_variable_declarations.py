"""
Cluster 07 - variable_declarations: 变量声明模糊测试
"""
import sys,os,random
sys.path.insert(0,os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

def generate(cluster_id,cluster_name,cluster_dir,fuzz_dir):
    reset_seed(); files=[]; cfg=[]
    for fi in range(110):
        reset_seed(); cn=unique_class_name(cluster_id,fi); b=FuzzClassBuilder(cn,cluster_name)
        for mi in range(random.randint(8,12)):
            mn=unique_method_name("test_decl",mi)
            dt=random.randint(0,5)
            if dt==0: body=["int x = 42;","print(\\\"int=\\\"); println(x);"]
            elif dt==1: body=['var x: int = 42;','print(\\\"var=\\\"); println(x);']
            elif dt==2: body=['let y: String = \\\"hello\\\";','print(\\\"let=\\\"); println(y);']
            elif dt==3: body=['auto z = 3.14;','print(\\\"auto=\\\"); println(z);']
            elif dt==4: body=['final int MAX = 100;','print(\\\"final=\\\"); println(MAX);']
            else: body=['final var X: int = 200;','print(\\\"final_var=\\\"); println(X);']
            b.add_method(mn,body+['println("OK");'])
        fn=f"fuzz_{fi:04d}.cay"; write_cay_file(fuzz_dir,fn,b.build())
        files.append({"filename":fn,"class":cn}); cfg.append({"file":fn,"class":cn})
    return files,cfg
