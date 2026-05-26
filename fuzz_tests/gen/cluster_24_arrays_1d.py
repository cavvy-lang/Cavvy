"""
Cluster 24 - arrays_1d: 一维数组模糊测试
"""
import sys,os,random
sys.path.insert(0,os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

def generate(cluster_id,cluster_name,cluster_dir,fuzz_dir):
    reset_seed(); files=[]; cfg=[]
    for fi in range(120):
        reset_seed(); cn=unique_class_name(cluster_id,fi); b=FuzzClassBuilder(cn,cluster_name)
        for mi in range(random.randint(8,14)):
            mn=unique_method_name("test_arr1",mi)
            n=random.randint(2,8)
            body=[f"int[] arr = new int[{n}];",
                f"for(int i=0;i<{n};i=i+1){{arr[i]=i*3+{random.randint(1,10)};}}",
                "for(int i=0;i<arr.length;i=i+1){println(arr[i]);}"]
            # 求和
            body+=["int sum=0;",
                "for(int i=0;i<arr.length;i=i+1){sum=sum+arr[i];}",
                'print(\\\"sum=\\\"); println(sum);']
            body+=['println("OK");']
            b.add_method(mn,body)
        fn=f"fuzz_{fi:04d}.cay"; write_cay_file(fuzz_dir,fn,b.build())
        files.append({"filename":fn,"class":cn}); cfg.append({"file":fn,"class":cn})
    return files,cfg
