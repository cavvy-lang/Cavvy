"""
Cluster 22 - break_continue: break/continue模糊测试
"""
import sys,os,random
sys.path.insert(0,os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

def generate(cluster_id,cluster_name,cluster_dir,fuzz_dir):
    reset_seed(); files=[]; cfg=[]
    for fi in range(100):
        reset_seed(); cn=unique_class_name(cluster_id,fi); b=FuzzClassBuilder(cn,cluster_name)
        for mi in range(random.randint(8,14)):
            mn=unique_method_name("test_brk",mi)
            n=random.randint(5,15)
            body=[f"for(int i=0;i<{n};i=i+1){{if(i=={n//2}) break; println(i);}}"]
            body+=['println("--");']
            body+=[f"for(int i=0;i<{n};i=i+1){{if(i%2==0) continue; println(i);}}"]
            body+=['println("OK");']
            b.add_method(mn,body)
        fn=f"fuzz_{fi:04d}.cay"; write_cay_file(fuzz_dir,fn,b.build())
        files.append({"filename":fn,"class":cn}); cfg.append({"file":fn,"class":cn})
    return files,cfg
