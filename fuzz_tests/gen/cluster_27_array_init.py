"""
Cluster 27 - array_init: 数组初始化器模糊测试
"""
import sys,os,random
sys.path.insert(0,os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

def generate(cluster_id,cluster_name,cluster_dir,fuzz_dir):
    reset_seed(); files=[]; cfg=[]
    for fi in range(100):
        reset_seed(); cn=unique_class_name(cluster_id,fi); b=FuzzClassBuilder(cn,cluster_name)
        for mi in range(random.randint(8,14)):
            mn=unique_method_name("test_ainit",mi)
            n=random.randint(3,8)
            vals=','.join(str(random.randint(1,999)) for _ in range(n))
            body=[f"int[] arr = {{{vals}}};",
                f"print(\\\"len=\\\"); println(arr.length);",
                "for(int i=0;i<arr.length;i=i+1){println(arr[i]);}"]
            # 2d init
            body+=["int[][] m2 = {{1,2,3},{4,5,6}};",
                'println(m2[0][0]); println(m2[1][2]);']
            body+=['println("OK");']
            b.add_method(mn,body)
        fn=f"fuzz_{fi:04d}.cay"; write_cay_file(fuzz_dir,fn,b.build())
        files.append({"filename":fn,"class":cn}); cfg.append({"file":fn,"class":cn})
    return files,cfg
