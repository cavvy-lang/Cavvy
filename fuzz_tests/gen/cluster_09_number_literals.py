"""
Cluster 09 - number_literals: 数字字面量模糊测试
"""
import sys,os,random
sys.path.insert(0,os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

def generate(cluster_id,cluster_name,cluster_dir,fuzz_dir):
    reset_seed(); files=[]; cfg=[]
    LITS=[("hex","0xFF","255","int"),("hex","0xABCD","43981","int"),("hex","0x0","0","int"),
          ("bin","0b1010","10","int"),("bin","0b11111111","255","int"),("bin","0b0","0","int"),
          ("bin","0b1","1","int"),("oct","0o77","63","int"),("oct","0o0","0","int"),
          ("oct","0o377","255","int"),("dec","1_000_000","1000000","int"),
          ("dec","1_000","1000","int"),("dec","0","0","int"),
          ("long","1_000_000_000L","1000000000","long"),("long","0L","0","long"),
          ("float","3.14159f","3","float"),("float","1.0f","1","float"),
          ("double","2.718281828459045","2","double"),("double","1e10","10000000","double"),
          ("hex","0x7FFFFFFF","2147483647","int")]
    for fi in range(100):
        reset_seed(); cn=unique_class_name(cluster_id,fi); b=FuzzClassBuilder(cn,cluster_name)
        for mi in range(random.randint(8,14)):
            mn=unique_method_name("test_lit",mi)
            lt,lit,exp,tp=random.choice(LITS)
            b.add_method(mn,[f"{tp} val = {lit};",f'print(\\\"{lit}=\\\"); println(val);','println("OK");'])
        fn=f"fuzz_{fi:04d}.cay"; write_cay_file(fuzz_dir,fn,b.build())
        files.append({"filename":fn,"class":cn}); cfg.append({"file":fn,"class":cn})
    return files,cfg
