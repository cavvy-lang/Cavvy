"""
Cluster 06 - string_methods: 字符串方法模糊测试
"""
import sys,os,random
sys.path.insert(0,os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from gen_utils import *

SAMPLES = ['"hello"','"world"','"Cavvy"','""','"abc"','"  hi  "','"Hello"','"A"','"bb"','"abcd"','"xyx"','"TestString"']

def generate(cluster_id,cluster_name,cluster_dir,fuzz_dir):
    reset_seed(); files=[]; cfg=[]
    for fi in range(120):
        reset_seed(); cn=unique_class_name(cluster_id,fi); b=FuzzClassBuilder(cn,cluster_name)
        for mi in range(random.randint(10,16)):
            mn=unique_method_name("test_strm",mi)
            s=random.choice(SAMPLES)
            m=random.randint(0,13)
            if m==0: body=[f"String s={s};","print(\\\"len=\\\"); println(s.length());"]
            elif m==1: body=[f"String s={s};","String sub=s.substring(0,1);","println(sub);"]
            elif m==2: body=[f"String s={s};",'int idx=s.indexOf(\\\"a\\\");','println(idx);']
            elif m==3: body=[f"String s={s};",'String r=s.replace(\\\"a\\\",\\\"X\\\");','println(r);']
            elif m==4: body=[f"String s={s};","char c=s.charAt(0);","println(c);"]
            elif m==5: body=[f"String s={s};","String u=s.toLowerCase();","println(u);"]
            elif m==6: body=[f"String s={s};","String u=s.toUpperCase();","println(u);"]
            elif m==7: body=[f"String s={s};","String t=s.trim();","println(t);"]
            elif m==8: body=[f"String s={s};",'boolean st=s.startsWith(\\\"a\\\");','println(st);']
            elif m==9: body=[f"String s={s};",'boolean en=s.endsWith(\\\"z\\\");','println(en);']
            elif m==10: body=[f"String s={s};",'boolean co=s.contains(\\\"b\\\");','println(co);']
            elif m==11: body=[f"String s={s};",'boolean eq=s.equals(\\\"hello\\\");','println(eq);']
            elif m==12: body=[f"String s={s};","boolean em=s.isEmpty();","println(em);"]
            else: body=[f"String s={s};",'int cp=s.compareTo(\\\"abc\\\");','println(cp);']
            b.add_method(mn,body+['println("OK");'])
        fn=f"fuzz_{fi:04d}.cay"; write_cay_file(fuzz_dir,fn,b.build())
        files.append({"filename":fn,"class":cn}); cfg.append({"file":fn,"class":cn})
    return files,cfg
