"""
Cluster 54 - expressions_complex: 复杂嵌套表达式模糊测试
"""
import sys,os,random
sys.path.insert(0,os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

def generate(cluster_id,cluster_name,cluster_dir,fuzz_dir):
    reset_seed(); files=[]; cfg=[]
    for fi in range(110):
        reset_seed(); cn=unique_class_name(cluster_id,fi); b=FuzzClassBuilder(cn,cluster_name)
        for mi in range(random.randint(10,16)):
            mn=unique_method_name("test_expr",mi)
            a=random.randint(1,50); bv=random.randint(1,50)
            cv=random.randint(1,50); d=random.randint(1,10)
            exprs=[
                f"int r = ({a}+{bv})*{cv};",
                f"int r = {a}+{bv}*{cv};",
                f"int r = ({a}*{bv})+({cv}/{d});",
                f"int r = ({a}*{bv})%({cv}+{d});",
                f"int r = {a}*{bv}-{cv}/{d};",
                f"int r = ({a}+{bv})/({cv}%{d}+1);",
            ]
            body=[random.choice(exprs),
                'print(\\\"expr=\\\"); println(r);']
            body+=['println("OK");']
            b.add_method(mn,body)
        fn=f"fuzz_{fi:04d}.cay"; write_cay_file(fuzz_dir,fn,b.build())
        files.append({"filename":fn,"class":cn}); cfg.append({"file":fn,"class":cn})
    return files,cfg
