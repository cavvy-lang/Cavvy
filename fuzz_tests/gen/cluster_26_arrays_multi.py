"""
Cluster 26 - arrays_multi: 高维数组模糊测试
"""
import sys,os,random
sys.path.insert(0,os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

def generate(cluster_id,cluster_name,cluster_dir,fuzz_dir):
    reset_seed(); files=[]; cfg=[]
    for fi in range(100):
        reset_seed(); cn=unique_class_name(cluster_id,fi); b=FuzzClassBuilder(cn,cluster_name)
        for mi in range(random.randint(8,12)):
            mn=unique_method_name("test_am",mi)
            d1=random.randint(2,3); d2=random.randint(2,3); d3=random.randint(2,3)
            body=[f"int[][][] arr = new int[{d1}][{d2}][{d3}];",
                f"for(int i=0;i<{d1};i=i+1){{",
                f"  for(int j=0;j<{d2};j=j+1){{",
                f"    for(int k=0;k<{d3};k=k+1){{",
                f"      arr[i][j][k]=i*100+j*10+k+1;",
                "    }",
                "  }",
                "}",
                f"println(arr[0][0][0]);"]
            body+=['println("OK");']
            b.add_method(mn,body)
        fn=f"fuzz_{fi:04d}.cay"; write_cay_file(fuzz_dir,fn,b.build())
        files.append({"filename":fn,"class":cn}); cfg.append({"file":fn,"class":cn})
    return files,cfg
