"""
Cluster 08 - type_casting: 类型转换模糊测试
"""
import sys,os,random
sys.path.insert(0,os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

def generate(cluster_id,cluster_name,cluster_dir,fuzz_dir):
    reset_seed(); files=[]; cfg=[]
    CASTS=[("int","double","3.14","(int)"),("double","int","42","(double)"),
           ("int","long","100L","(int)"),("long","int","42","(long)"),
           ("float","int","42","(float)"),("int","float","3.14f","(int)"),
           ("char","int","65","(char)"),("int","char","'A'","(int)")]
    for fi in range(100):
        reset_seed(); cn=unique_class_name(cluster_id,fi); b=FuzzClassBuilder(cn,cluster_name)
        for mi in range(random.randint(8,12)):
            mn=unique_method_name("test_cast",mi)
            td,ts,val,co=random.choice(CASTS)
            b.add_method(mn,[f"{ts} src = {val};",f"{td} dst = {co}src;",
                'print(\\\"cast=\\\"); println(dst);','println("OK");'])
        fn=f"fuzz_{fi:04d}.cay"; write_cay_file(fuzz_dir,fn,b.build())
        files.append({"filename":fn,"class":cn}); cfg.append({"file":fn,"class":cn})
    return files,cfg
