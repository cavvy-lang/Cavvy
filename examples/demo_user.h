/* 演示 #include_c 兜底路径：真实 C 头文件（无 .cay 包装），保守提取器解析。
 *
 * 这里故意只声明两个已经默认链接进每个 Cay 程序的 C 运行时函数（abs/rand），
 * 而不是发明全新符号 —— 这样不需要额外编译/链接自定义 .c 实现，
 * 也能验证「引号真实头 -> 保守提取 -> 自动映射类型 -> 可直接调用」这条兜底路径。
 */
#ifndef DEMO_USER_H
#define DEMO_USER_H

int abs(int n);
int rand(void);

#endif
