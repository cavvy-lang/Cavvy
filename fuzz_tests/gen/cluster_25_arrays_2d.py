"""
Cluster 25 - arrays_2d: 二维数组模糊测试
"""
import sys,os,random
sys.path.insert(0,os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

def generate(cluster_id,cluster_name,cluster_dir,fuzz_dir):
    reset_seed(); files=[]; cfg=[]
    for fi in range(110):
        reset_seed(); cn=unique_class_name(cluster_id,fi); b=FuzzClassBuilder(cn,cluster_name)
        for mi in range(random.randint(8,14)):
            mn=unique_method_name("test_arr2",mi)
            r=random.randint(2,4); c=random.randint(2,4)
            body=[f"int[][] m = new int[{r}][{c}];",
                f"for(int i=0;i<{r};i=i+1){{for(int j=0;j<{c};j=j+1){{m[i][j]=i*{c}+j+1;}}}}",
                f"for(int i=0;i<{r};i=i+1){{for(int j=0;j<{c};j=j+1){{println(m[i][j]);}}}}"]
            body+=['println("OK");']
            b.add_method(mn,body)
        fn=f"fuzz_{fi:04d}.cay"; write_cay_file(fuzz_dir,fn,b.build())
        files.append({"filename":fn,"class":cn}); cfg.append({"file":fn,"class":cn})
    return files,cfg
