"""
Cluster 18 - for_loop: for循环模糊测试
"""
import sys,os,random
sys.path.insert(0,os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

def generate(cluster_id,cluster_name,cluster_dir,fuzz_dir):
    reset_seed(); files=[]; cfg=[]
    for fi in range(110):
        reset_seed(); cn=unique_class_name(cluster_id,fi); b=FuzzClassBuilder(cn,cluster_name)
        for mi in range(random.randint(8,14)):
            mn=unique_method_name("test_for",mi)
            n=random.randint(1,10); step=random.choice([1,2,-1])
            if step==-1:
                body=[f"for(int i={n};i>0;i=i{step}){{println(i);}}"]
            else:
                body=[f"for(int i=0;i<{n};i=i+{step}){{println(i);}}"]
            # nested for
            n2=random.randint(1,4)
            body+=[f"for(int j=0;j<{n2};j=j+1){{for(int k=0;k<{n2};k=k+1){{println(j*10+k);}}}}"]
            body+=['println("OK");']
            b.add_method(mn,body)
        fn=f"fuzz_{fi:04d}.cay"; write_cay_file(fuzz_dir,fn,b.build())
        files.append({"filename":fn,"class":cn}); cfg.append({"file":fn,"class":cn})
    return files,cfg
