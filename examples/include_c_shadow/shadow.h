/* 回归测试：引号形式 #include_c 必须解析真实头文件，不得被同名 .cay 劫持。 */
#ifndef INCLUDE_C_SHADOW_H
#define INCLUDE_C_SHADOW_H

int abs(int n);

#endif
