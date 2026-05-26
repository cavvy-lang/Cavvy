"""
Cluster 13 - logical_operators: 逻辑运算符组合模糊测试
"""
import sys,os,random
sys.path.insert(0,os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

def generate(cluster_id,cluster_name,cluster_dir,fuzz_dir):
    reset_seed(); files=[]; cfg=[]
    for fi in range(100):
        reset_seed(); cn=unique_class_name(cluster_id,fi); b=FuzzClassBuilder(cn,cluster_name)
        for mi in range(random.randint(8,14)):
            mn=unique_method_name("test_logic",mi)
            tv=["true","false"]
            a,bv,cv=random.choice(tv),random.choice(tv),random.choice(tv)
            r=random.randint(0,5)
            if r==0: expr=f"boolean r = {a} && {bv};"
            elif r==1: expr=f"boolean r = {a} || {bv};"
            elif r==2: expr=f"boolean r = !({a});"
            elif r==3: expr=f"boolean r = ({a} && {bv}) || {cv};"
            elif r==4: expr=f"boolean r = {a} && ({bv} || {cv});"
            else: expr=f"boolean r = !({a} && {bv}) || {cv};"
            b.add_method(mn,[expr,'print(\\\"logic=\\\"); println(r);','println("OK");'])
        fn=f"fuzz_{fi:04d}.cay"; write_cay_file(fuzz_dir,fn,b.build())
        files.append({"filename":fn,"class":cn}); cfg.append({"file":fn,"class":cn})
    return files,cfg
