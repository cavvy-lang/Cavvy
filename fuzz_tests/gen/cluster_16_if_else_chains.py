"""
Cluster 16 - if_else_chains: if/else链模糊测试
"""
import sys,os,random
sys.path.insert(0,os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

def generate(cluster_id,cluster_name,cluster_dir,fuzz_dir):
    reset_seed(); files=[]; cfg=[]
    for fi in range(110):
        reset_seed(); cn=unique_class_name(cluster_id,fi); b=FuzzClassBuilder(cn,cluster_name)
        for mi in range(random.randint(10,16)):
            mn=unique_method_name("test_if",mi)
            v=random.randint(-50,50)
            body=[f"int x = {v};"]
            if v>10: body+=['if(x>10){println("big");}']
            elif v>0: body+=['if(x>0){println("pos");}else{println("nonpos");}']
            elif v==0: body+=['if(x==0){println("zero");}']
            else: body+=['if(x<0){println("neg");}else{println("nonneg");}']
            # second conditional
            v2=random.randint(-50,50)
            body+=[f"int y = {v2};",
                'if(y>0 && x>0){println("both pos");} else if(y<0 && x<0){println("both neg");} else {println("mixed");}']
            body+=['println("OK");']
            b.add_method(mn,body)
        fn=f"fuzz_{fi:04d}.cay"; write_cay_file(fuzz_dir,fn,b.build())
        files.append({"filename":fn,"class":cn}); cfg.append({"file":fn,"class":cn})
    return files,cfg
