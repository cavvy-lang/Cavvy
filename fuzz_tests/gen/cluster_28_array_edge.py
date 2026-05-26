"""
Cluster 28 - array_edge: 数组边界和边缘模糊测试
"""
import sys,os,random
sys.path.insert(0,os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

def generate(cluster_id,cluster_name,cluster_dir,fuzz_dir):
    reset_seed(); files=[]; cfg=[]
    for fi in range(100):
        reset_seed(); cn=unique_class_name(cluster_id,fi); b=FuzzClassBuilder(cn,cluster_name)
        for mi in range(random.randint(8,14)):
            mn=unique_method_name("test_aedge",mi)
            n=random.choice([0,1,2,10,100])
            body=[f"int[] arr = new int[{n}];",
                f"print(\\\"len=\\\"); println(arr.length);",
                f"for(int i=0;i<arr.length;i=i+1){{arr[i]=i*7;}}"]
            if n>0:
                body+=["print(\\\"first=\\\"); println(arr[0]);",
                    "print(\\\"last=\\\"); println(arr[arr.length-1]);"]
            # negative test: create zero-size and check length
            body+=["int[] arr2 = new int[0];",
                'print(\\\"zero_len=\\\"); println(arr2.length);']
            body+=['println("OK");']
            b.add_method(mn,body)
        fn=f"fuzz_{fi:04d}.cay"; write_cay_file(fuzz_dir,fn,b.build())
        files.append({"filename":fn,"class":cn}); cfg.append({"file":fn,"class":cn})
    return files,cfg
