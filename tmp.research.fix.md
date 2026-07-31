# Cavvy 代码库生产条件审查报告

> 审查日期：2026-07-26 · 审查范围：全仓库（src/ 84,751 行 / 159 个 .rs 文件 + 根目录工程化）
> 方法：8 路并行只读审查（lexer/preprocessor、parser/ast、semantic/types、codegen/ir、bin/lib/诊断、cavly/bytecode/rcpl、根目录卫生、横向风险扫描），部分发现已实测复现。

---

## 一、总览：最危险的系统性问题

这个代码库的核心病灶不是个别 bug，而是一种**"静默回退"文化**：出错时不报错，而是返回默认值继续——字段偏移找不到返回 0、类大小未知按 8 字节分配、宏展开失败当 false、类型推断失败兜底 Int32、包完整性校验结果直接 `let _ =` 丢弃。对编译器和包管理器而言，这把**编译期错误推迟成运行期内存损坏和供应链漏洞**，是最不符合生产条件的模式。

### 严重度分布

- 🔴 **P0 构建断裂**：1 项（当前 HEAD 无法全新构建）
- 🔴 **P1 正确性缺陷（silent miscompile 级）**：约 15 项
- 🔴 **P2 安全缺陷**：约 8 项
- 🟡 **P3 半成品/假功能**：4 个子系统（bytecode 全链路、混淆器 ×2、JIT、cavly 安全链）
- ⚪ **屎山信号**：901 行函数、71 字段 god object、估计 1500+ 行复制粘贴、~400 行自认死代码

---

## 二、P0：当前 HEAD 构建断裂（最紧急）

### 1. `.verinfo` 被"版本升级"提交清空，全新构建必然失败
- 提交 `bf00f97`「feat: 更新版本号至 6.2.0」把 `.verinfo` 的 69 行**全部删空**（git show --stat 证实：69 deletions, 0 insertions），当前磁盘和 HEAD 中均为 0 字节。
- 后果链：`build.rs:151-186` 的 `parse_verinfo()` 对空文件返回 `Ok(空 map)`（不报错），16 个 `if let Some(...)` 全部落空，**不发任何 `cargo:rustc-env`**；而 14 个 bin 全部 `env!("CAYC_VERSION")`（如 `src/bin/cayc.rs:14`）——`env!` 在变量未定义时是编译期硬错误。任何 fresh clone / `cargo clean` 后构建必挂。
- 现有 `target/release/cayc` 能跑只是因为它是旧缓存产物（报 `v6.2.2+d1bf8c1-dirty`，落后 HEAD 两个提交）。
- 雪上加霜：`build.rs:360` 的 Err 分支 fallback 硬编码 `5.1.0-Alpha.3`（只在"读文件失败"时触发，对"文件为空"无效）。

### 2. 版本号五处互相矛盾
| 位置 | 版本 |
|---|---|
| `Cargo.toml:4` | 6.2.0（且是从 6.2.2 **降级**的） |
| HEAD~1 的 `.verinfo` | 6.2.2 |
| `AGENTS.md` | 5.1.0-Beta.2 |
| `build.rs` fallback | 5.1.0-Alpha.3 |
| `CHANGELOG.md` 最新条目 | [6.1.0]（[Unreleased] 为空） |

---

## 三、P1：正确性缺陷（会静默产出错误程序）

### 编译器前端

1. **内联 IR 静默丢弃 token —— 已实测的 silent miscompile** 🔴
   `src/parser/statements.rs:1192-1194`：`parse_inline_ir_from_tokens` 兜底分支 `_ => { parser.advance(); }` 静默吞掉白名单外的 token。实测：`__ir { %neg = add i32 -5, 0 }` 生成了 `%neg = add i32 5, 0`（负号被吞）。受害 token 还包括 `:`（label）、`!`（metadata）、字符串字面量。

2. **`(a) + b` 无法解析 —— 已实测** 🔴
   `src/parser/expressions/unary.rs:83-119`：cast 预读看到 `(Identifier)` 就无条件 commit 为类型转换且不回退。`int c = (a) + b;` 报 `[E3001] 期望表达式，但遇到了 Plus`。

3. **`#if !defined(FOO)` / `#if A&&B` 静默算错**
   `src/preprocessor/mod.rs:1516-1530`：`consume_keyword` 被复用于消费 `!`、`&&`、`||`，但要求关键字后必须是非字母数字字符 → 无空格写法消费失败 → `evaluate_condition` 走 `Err(_) => false`（`:682`），分支被静默丢弃。所有单测都带空格所以从未暴露。

4. **`c_uint64_t` 被映射为有符号 `Int64`**
   `src/parser/types.rs:136-143`：`CUInt64 => Type::Int64`。FFI 中值 > i64::MAX 时被当负数。

5. **宏替换不感知字符串 + 每字符重建宏表**
   `src/preprocessor/mod.rs:1252-1286`：`expand_macros` 在字符级循环**内部**对每个字符收集并 `sort_by` 整个宏表（O(行长×宏数 log 宏数)），且 `"PI"` 这类字符串字面量内容也会被宏替换。同文件 `remove_line_comments`（`:1108`）会把 `#define URL http://x.com` 的值截断成 `http:`。

6. **超大整数字面量静默退化**
   `src/lexer/mod.rs:229-233`：`parse::<i64>().ok()` 溢出得 `IntegerLiteral(None)`，parser 7 处只匹配 `Some(...)`，用户得到莫名其妙的"unexpected token"。`LexerErrorType::InvalidNumberLiteral` 定义了却从未被构造。

7. **类型系统根基问题：null 与 Object 共用类型表示**
   `src/semantic/expr_inference/identifier.rs:42`：`LiteralValue::Null => Type::Object("Object")`；`src/semantic/type_utils.rs:145-150` 对 `Object("Object")` 无条件兼容任何类型 → **真正的 Object 实例可赋给 String/任何类/任何接口而不报错**。

8. **比较运算符完全不检查操作数类型**
   `src/semantic/expr_inference/binary.rs:94-99`：`Eq|Ne|Lt|Le|Gt|Ge => Ok(Type::Bool)`，`"abc" < 42` 直接漏到 codegen。

9. **codegen 的静默回退三连**（编译器最恶劣的 bug 类型）
   - `src/codegen/generator.rs:2735-2747`：字段偏移查找失败 `eprintln!` 一行后 `Ok(0)` —— 字段被写到对象头（this+0），注释自己写着"不应发生"。
   - `src/ir/builder.rs:2199`：类大小未知 `unwrap_or(8)` —— 字段多的类直接堆溢出。
   - `src/ir/builder.rs:1886-1934`：方法签名解析失败回退成字面量 `"x"` 占位（拼出 `Class.__method_x_x`），**同段代码复制三遍**，最终以链接错误收场，用户无法定位。

10. **`check_next` 差一错误 + usize 下溢**
    `src/parser/utils.rs:259`：`parser.pos + 1 >= parser.tokens.len() - 1`，`tokens.len()==0` 时下溢 wrap。

11. **语义分析的吞错 + Int32 兜底**
    `src/semantic/expr_inference/identifier.rs:12-26`：推断失败返回 `Type::Int32` 继续；`get_error_location` 返回 None 时错误被**完全丢弃**。重载决议的试探性调用（`type_utils.rs:616-915`）污染全局错误列表，失败候选的错误不回滚 → 级联误报。

12. **数组初始化只检查第一个元素**
    `src/semantic/expr_inference/array.rs:132-134`：`{1, "hello", true}` 以第一个元素类型通过。

13. **@Test 校验错误信息嵌入字面 Rust 代码**（用户可见的低级 bug）
    `src/semantic/class_analysis.rs:418,431,446`：format 字符串里写着 `... public void {}(, ErrorCodes::get_suggestion(...)"`，插值调用被写进了字符串字面量。

14. **字节码生成静默产出错误程序**
    `src/bin/cay-bcgen.rs`：`break` 编译成 `Opcode::Return`（`:553`，提前退出函数）；`continue` 生成零指令（`:557`，while 变死循环）；变量查不到时 `iload(0)`（`:718`）；方法体失败被 `.ok().flatten()` 吞掉退化为空方法体（`:271`）。

15. **cay-run --obfuscate 生成空程序**
    `src/bin/cay-run.rs:355-364`：跑完前端分析后只创建空 `BytecodeModule::new()`，函数体从未写入（`:364` TODO 原样承认），用户加 `--obfuscate` 后得到不含任何代码的可执行文件，无任何报错。

---

## 四、P2：安全缺陷

1. **cavly 官方根公钥永远为 None，"双重签名"验证名存实亡** 🔴
   `src/cavly/security.rs:485-489`：`official_root_public_key()` 硬编码返回 `None`，authority 签名永远跳过验证；发布者签名公钥又来自同一台服务器下发的 meta——**整个 ESSO-10430 安全模型没有信任锚，空转**。

2. **本地包完整性校验结果被 `let _ =` 丢弃** 🔴
   `src/cavly/workspace.rs:310`：注释写着"安全验证"，下一行 `let _ = self.verify_local_package_if_possible(...)` 丢弃结果，哈希不匹配/证书无效照样链接使用。

3. **PowerShell 下载分支命令注入** 🔴
   `src/cavly/registry.rs:441-447`：URL 直接插值进 PowerShell `-Command` 字符串（单引号包裹但不转义），URL 含 `'` 即注入。URL 来源含用户 `cavly add --source` 和索引服务器下发字段。curl/wget 分支走 `.args()` 是安全的，唯独 PS 分支是字符串拼接。

4. **指纹未验证直接拼缓存路径 → 路径遍历**
   `src/cavly/registry.rs:366,376,385,394`：服务器索引返回的 fingerprint 未经 UUID 校验直接 `cache_dir.join(format!("{}.json", fingerprint))`，恶意索引返回 `../../x` 可越目录写文件。

5. **`curl -sL` 不带 `--fail`，404 页面被当成功数据**
   `src/cavly/registry.rs:421-435`：404/500 的 HTML 错误页被当作合法包数据写盘，fallback 策略永远不执行，最后以误导性的"SHA-256 校验失败"报错。

6. **反序列化对攻击者可控长度直接 `with_capacity` → OOM**
   `src/bytecode/serializer.rs`：15+ 处 `Vec::with_capacity(len)`，len 是文件里的 u32，4 字节 `0xFFFFFFFF` 即触发数十 GB 预分配 abort。另 `deserialize`（`:384`）版本不匹配时静默继续解析。

7. **每个 line==0 的错误都在用户 CWD 倾倒含完整源码的调试文件** 🔴
   `src/miette_diagnostic.rs:1316-1363`（`emit_zero_line_debug_info`）：所有 Io 错误（`location()` 恒为 None）都会静默 `fs::write("debug_{code}_{timestamp}.txt", 含完整源代码)`——生产环境污染用户目录 + **泄露源代码**，写入失败还被静默吞掉。

8. **其他**
   - 证书过期时间从不校验：`src/cavly/security.rs:163`（`expires_at` 只解析不检查；`audit.rs:41` 的 `CachedCertificateExpired` 事件定义了无人使用）。
   - `find_clang`/`find_llc`/`find_lld` **PATH 优先**、捆绑工具链兜底（`src/ir2exe_lib.rs:322-354` + `src/bytecode/jit.rs:1397` 两份复制粘贴）：非 hermetic，PATH 劫持场景执行攻击者放置的"clang"。
   - `src/ir/inline_ir.rs:103`：`new_unsafe()` 后门 API 可绕过整个指令白名单。
   - `src/bin/cavly.rs:623`：`AuditLogger::new().unwrap_or_default()`，安全审计日志初始化失败静默吞掉。

---

## 五、P3：半成品 / 名不副实的子系统

### 1. bytecode 全链路处于 demo 级完成度，但已接入发布二进制
- **混淆器是假功能**：`src/bytecode/obfuscator.rs` —— 名称混淆循环体只有 `let _ = original; let _ = obfuscated;`（`:162-180`，唯一实效是 `header.obfuscated = true`）；字符串加密只写密钥不加密（`:282-303`）；控制流混淆**破坏语义**——在条件跳转前插入 `iconst(0); Iadd`，`Ifeq` 测试的是恒真的 0 而非原条件（`:199-236`），混淆后程序行为是错的。
- **JIT 生成大面积非法 LLVM IR**：`src/bytecode/jit.rs`（1439 行，被 `cay-run:417` 实际调用）——硬编码 `target triple = "x86_64-pc-windows-gnu"` 无视用户目标（`:265`）；internal linkage 缺 `define` 关键字（`:443`）；结构体字段列表带尾逗号（`:369`）；字符串转义用 LLVM 不支持的 `\"` 且长度算错（`:337`）；浮点常量格式非法（`:1299`）；未实现 opcode 走 `_ =>` 只生成一行注释**静默丢指令**（`:1227`）；`Opcode::New` 硬编码 `malloc(i64 64)`（`:1221`）。文件头自称 "JIT" 实际是 bytecode→IR 文本→clang。
- **建议**：整条 `cay-run --bytecode` / `cay-bcgen` / `--obfuscate` 链路要么修复（工作量大），要么显式标记实验性并默认禁用。

### 2. IR 混淆器会破坏字符串字面量（已实测复现）
`src/codegen/obfuscator.rs:61-90`：对任何含 `@` 的非注释行做符号替换，不区分符号引用和字符串内容。实测 `c"email: a@b.com\00"` → `c"email: a@__obf_1\00"`——既篡改程序输出，又因字节数与 `[16 x i8]` 声明长度不符被 clang 拒收。经 `cay-ir --obfuscate` 可达。**修好前应禁用**。

### 3. RCPL
`src/rcpl/mod.rs:144-151`：EOF（Ctrl-D）返回 `Ok(0)` 不算错误 → 无限刷提示符的忙等死循环；`:386` `Default::default()` 里 `.expect()` 直接 panic；`:325` 临时文件用 PID 命名可预测。

### 4. 其他半成品
- `src/cavly/builder.rs:424-428`：`let shell = if cfg!(windows) { "cmd" } else { "cmd" };` —— 两分支相同，非 Windows 必失败的死代码。
- `src/preprocessor/mod.rs:592`：`// TODO: 实现完整的条件表达式评估` —— 下方 400+ 行的 `ConditionParser` 就是完整实现，TODO 是残留（或反过来：实现有缺陷，见 P1-3）。
- `src/cavly/workspace.rs:139`：registry 版本依赖解析未实现（"目前仅支持本地路径依赖"）。
- parser 的 `diagnostics` 收集（`utils.rs:189/215/253` 每次 push）全仓库无人读取；`synchronize` 错误恢复函数从未被调用；`semantic/type_inference_result.rs`（167 行）整套抽象零调用点。
- `src/bin/cay-pre.rs` 存在但 Cargo.toml 无对应 `[[bin]]`（`autobins=false`）——**永远无法构建的死文件**；`src/main.rs`（68 行）同。
- `miette_diagnostic.rs:1459-1862`：约 400 行 `CavvyError`/`CavvyWarning`/`LexerError` 等 miette-derive 类型标注"保留供未来使用"，零引用。

---

## 六、🟡 中等严重度问题（选录）

### 编译器
- `src/parser/statements.rs:887,1074`：内联 IR 中 `IntegerLiteral(None)` 静默变 `"0"`。
- `src/ast.rs:846-852`：同名嵌套 namespace 被静默"去重"（`namespace a { namespace a {} }` 路径变 `a` 而非 `a::a`）——为掩盖别处 bug 打的补丁本身制造静默错误。
- `src/parser/classes.rs:1060`：struct 方法缺尾表达式提升，与 class 方法行为不一致。
- `src/parser/classes.rs:747`：`public private static static int x;` 重复/冲突修饰符全部照收。
- `src/parser/expressions/lambda.rs:163`：lambda 块位置信息伪造为 line 0，诊断指向不存在的位置。
- `src/types.rs:2060`：`find_implementing_class_for_method` 遍历 HashMap 返回第一个匹配——多类实现同一接口时解析结果**不确定**。
- `src/types.rs:1969` / `type_utils.rs:475`：类名兼容性在两查不到时退化为比较简单名，不同命名空间同名类被判兼容。
- `src/types.rs:753`：`size_in_bytes` 对 `Type::Auto` 直接 `unreachable!`（生产路径共 ~7 处 `unreachable!`/`panic!`，内部不一致时编译器裸 panic 而非诊断）。
- `src/types.rs:809`：`Type::is_integer` 疑似残缺（只有 Int32|Int64|CULong），与同文件 `is_numeric_type`（17 种）口径矛盾，导致位运算/数组下标错误拒绝 Char/SizeT 等。
- 循环继承防护不一致：`type_utils.rs:399` 有防环，`helpers.rs:84`、`member_access.rs:338`、`class_analysis.rs:1217` 等父链遍历均无防环，靠前置检查兜底。
- parser/semantic **全库无递归深度限制**（仅 `ir/inliner.rs:20` 有）——病态嵌套输入（10 万层括号）栈溢出直接 crash，处理不可信源码时是 DoS 面。
- `src/preprocessor/mod.rs:1407`：`bundled_c_include_paths()` 写死 `llvm-minimal/lib/clang/21/include`——实际捆绑 clang-22 且扁平布局，该路径**永远不存在**，是死 fallback。
- `src/preprocessor/mod.rs:625,901`：库代码绕过诊断体系直接 `eprintln!`；`src/codegen/generator.rs:2742`、`inliner.rs:114` 同。
- `src/codegen/context.rs:417`：`IRGenerator::new()` 硬编码默认目标 `x86_64-w64-mingw32`，Linux 上默认得到 Windows 目标；与 `runtime/mod.rs:35` 的 `cfg!(target_os)` 探测两套逻辑并存。
- `src/lexer/mod.rs:800-822`：`collect_all_errors` 标志两分支逐行相同——死开关。
- `src/lexer/mod.rs:205-233`：前导零规则自相矛盾（正则归八进制形态、radix 按十进制解析、`08` 词法错误），注释自己写糊涂了。

### CLI / 工具
- `src/bin/cay-lsp.rs:260`：`&line[..position.character.min(line.len() as u32) as usize]` —— LSP character 是 UTF-16 单元而切片按字节，中文行光标落在多字节字符中间时 **panic**；且该变量是死代码。本项目中文注释极多，触发概率高。
- `src/bin/cay-lsp.rs:941`：文档符号行号未做 1-based→0-based 转换，全部偏移一行。
- `src/bin/cay-dt.rs:248-293`：手写 JSON 拼接带尾随逗号——输出非法 JSON（其他 bin 用 serde_json，这是过时残留）。
- `src/bin/cay-ir.rs:25`：捆绑 clang 硬编码 `llvm-minimal/bin/clang.exe`——Linux/macOS 永远找不到。
- `src/lib.rs:113-121,251-270`：`#[cfg(debug_assertions)]` 块无条件向 **stdout** 打印全部 token 流——debug 构建下污染 LSP 的 JSON-RPC 通道。
- `src/bin/cayc.rs:672` / `cay-run.rs:486`：用 `ir_content.contains("socket(")` 字符串匹配猜测是否链 `ws2_32`；`src/cavly/linker.rs:107` 同类启发式。
- `src/bin/cay-run.rs:558`：程序被信号杀死时 `unwrap_or(1)` 伪装退出码 1。
- `src/miette_diagnostic.rs:515`：`severity()` 靠中文字符串 `kind == "致命错误"` 判断 Fatal——魔法字符串。
- `src/miette_diagnostic.rs:1210`：高亮 span 长度 char/byte 混用，多字节标识符高亮错位。

---

## 七、屎山信号（结构债）

### 巨型文件与函数
- 24 个文件超 1000 行。最大：`codegen/generator.rs` 3945 行、`codegen/context.rs` 3270、`ir/builder.rs` 2625、`types.rs` 2143、`preprocessor/c_header.rs` 2050、`semantic/` 合计 7600。
- 最长函数实测：
  - `generate_call_expression`（`codegen/expressions/call/main.rs:15`）— **901 行，最大嵌套 16 层**
  - `infer_call_type`（`semantic/expr_inference/call.rs:11`）— 696 行
  - `generate_instruction`（`bytecode/jit.rs:573`）— 664 行
  - `compute_layout_recursive`（`codegen/generator.rs:220`）— 520 行
  - `parse_primary`（`parser/expressions/primary.rs:53`）— 458 行
  - `parse_inline_ir_from_tokens`（`parser/statements.rs:843`）— 366 行（含 ~40 个逐字相同的 match 臂）
- `IRGenerator` 是 **71 字段的 god object**（`context.rs:285`），`impl IRGenerator` 块 58 个、散布 20+ 文件。

### 复制粘贴（粗估 1500+ 行）
- `types.rs`：`StructInfo` 与 `ClassInfo` 的五连方法（find_method/types_match_exact/...）逐行重复各 ~220 行。
- parser：方法解析 4 份近亲实现、顶层分发 2 份、构造函数 2 份、`binary.rs` 8 个优先级函数逐字复制、近千行 stringly 诊断 match。
- 类型兼容性三套互不一致的规则表：`is_valid_cast`（cast.rs:61，135 行）/ `types_compatible`（type_utils.rs:134，218 行）/ `types_match_with_namespace`（types.rs:1929），且参数顺序约定混乱极易传反。
- `build.rs:193-348`：16 段逐字相同的版本环境变量块（~160 行，一个循环+名称表即可）。
- CLI：cayc.rs 与 ir2exe.rs 的参数解析/用法打印大面积重复；cay-dp（旧）与 cay-ast（新）功能完全重叠并存；cay-pl/cay-sir 各藏一份 `collect_inline_ir_*` 复制版；`compile()` 与 `compile_with_source_map_and_link_libs()`（lib.rs）70 行近乎逐字重复；ir2exe_lib.rs 两条平行编译管线（379 行 vs 393 行）。
- `generator.rs`：8 处完全相同的 `split("::").last().expect(...)` 块。

### 死代码与残留
- 生产代码 unwrap/expect ~37 处（纪律尚可）；`todo!`/`unimplemented!` **0 处**；硬编码绝对路径 0 处；unsafe 33 处集中 FFI（基本合理）。
- **53 处注释掉的 `// eprintln!` 调试残留**（builder.rs 1845-1935 区间几乎每个分支一条；embedded_llc.rs 有双层注释的 `// // eprintln!`）。
- 库模块 println!/eprintln! 共 996 处匹配（大部分在 CLI bin 属合理，但 embedded_llc.rs 有 33 处活跃输出）。
- 死代码选录：`src/main.rs`、`src/bin/cay-pre.rs`、`Parser::diagnostics`（只写不读）、`utils::is_type_token` 与 `types::is_type_token` 同名不同义（前者无人调用，纯地雷）、`utils::synchronize`、`Program::find_main_class`（逻辑已过时）、`type_inference_result.rs` 整文件、miette_diagnostic.rs 尾部 400 行、`cay-dt.rs:320` 死函数 `is_error`、词法错误 4 个变体从未被构造、cast.rs 重复 match 臂（`(CShort,Int32)` 出现两次）。
- `lexer/mod.rs:840-929`：`next_token()` 是 `tokenize()` 的 ~90 行复制粘贴，且错误消息英文、主路径中文——**全库错误消息中英文混用**（parser/semantic 同样存在）。
- CI 用 `RUSTFLAGS="-A warnings"` 压制所有警告 → 重复 match 臂、死变量、未使用导入长期隐身（unreachable_pattern 等本该暴露 P3 级问题）。

### 误导性注释 / 注释与代码不符
- `src/ir/mod.rs:5` 自称"生产级"；`bridge.rs:29` 声称"所有临时资源都有 RAII 管理"但该模块没有任何资源；`runtime/mod.rs:32` 生成的 IR 头注释仍写旧名 "Ethernos Object Language"。
- `src/preprocessor/mod.rs:3` 模块文档写"实现 0.3.5.0 版本"；代码注释里出现 0.5.0.0/5.3.0/6.1.0/6.2.x 多种版本记号，与 .verinfo 全对不上。
- `security.rs:436` 注释"验证 UUID v4"但调用处传 5；`obfuscator.rs:254` 注释与行为相反；`c_header.rs:1981` 测试名与断言行为相反。
- cavly/bytecode 大量形式主义"复杂度标注"（如网络操作标 "O(1)"），制造文档化假象。

---

## 八、工程卫生（根目录）

- **`AGENTS.md` 多处与事实不符**（误导所有后续 agent/贡献者）：声称存在 `.rs.bak.N` 备份文件（实际 0 个）；声称 anyhow "未使用"（实际 cavly/rcpl 等 12 个文件 74 处）；二进制数量写"11 个"（实际 Cargo.toml 15 个 [[bin]]）；引用的 `src/error.rs`、`src/diagnostic.rs`、`cayError` 不存在（实为 `miette_diagnostic.rs` 的 `CayError`/`CayResult`）；CI 的 `jekyll-gh-pages.yml` 不存在（现为 `docs.yml`）；版本号过时。
- **`build.rs` 是构建性能杀手**（631 行）：`rerun-if-changed=.git/HEAD` 和 `.git/index`（`:490`）——任何 git 操作触发 build.rs 重跑并因版本环境变量变化导致**全量重编**；每次构建把 580 个 examples 文件递归复制进 `target/<profile>/`（`:496,555`）；`:131` 调 `python` 而非 `python3`。
- **`examples/` 实为测试语料垃圾场**：571 个文件中 516 个是 `test_*.cay`（90%）+ 13 个 `bug_*`；残留 35 个文件名内嵌 `ThreadId(3)` 的 `*.ll.obj` 测试并发产物。
- **根目录杂物堆**（靠 .gitignore 反向模式遮羞）：`a.cay`、`a.ll`(692K)、`e.cay`、`err.cay`、`xswl.cay`（1 字节）、`debug_*.cay`×3、`hello*.exe`、`temp_*.exe`×5、`temp_std_algorithm_*.ll`×7（每个 709K）、日志 15+ 个、无后缀 ELF ×6。`.gitignore` 的"全局忽略再逐条反忽略"模式是杂物越积越多的制度性原因。
- `.gitmodules` 残留旧组织名 `Ethernos-Studio/ESSO`；根目录 `node_modules/` 装的是 opencode-ai 但无 package.json；`.sisyphus/`、`.trae/`、`.claude/` 三种 AI 工具痕迹并存；`.vscode/settings.json` 提交了个人 `rust-analyzer.trace.server: verbose`。
- 重依赖仅服务单个 bin：`regex`、`env_logger` 各只在 cay-lsp.rs 用一次。
- 提交信息质量差：如 `80887b1 "fex(build): 又又一个版本号格式写错"`（拼写错误+口语化），与版本混乱互为因果。

---

## 九、优先修复路线图

### 第一批（今天就修，改动小、止血）
1. **恢复 `.verinfo` 内容**，统一五处版本号；改 `build.rs` 让空文件也走兜底或硬报错。——不修则下一个 clone 的人构建即失败。
2. **拆除 `emit_zero_line_debug_info`**（miette_diagnostic.rs:1316）——生产环境往用户 CWD 倒含完整源码的调试文件。
3. **修 @Test 错误信息的字面代码串**（class_analysis.rs:418/431/446）——用户可见，改动极小。
4. **cavly 安全三件套**：`workspace.rs:310` 的 `let _ =` 改为硬错误；`registry.rs:442` PowerShell 分支转义/参数化；`http_get` 加 `--fail`；fingerprint 入路径前 UUID 校验。

### 第二批（正确性，需要设计但收益最大）
5. **消灭 codegen 静默回退**：`get_field_offset` 返回 0、`unwrap_or(8)`、`"x"` 签名回退全部改硬错误。原则：**编译器宁可 noisy，不可 silently wrong**。
6. **修内联 IR token 白名单**（statements.rs:1192）：补 Minus/Bang/Colon 等，不认识的 token 报错而非丢弃——已实测的 silent miscompile。
7. **修 cast 预读不回退**（unary.rs:83）——`(a) + b` 合法表达式被拒。
8. **修 `#if` 表达式**：`consume_keyword` 对操作符的误用 + `Err(_) => false` 至少发警告。
9. **禁用或修复两个混淆器**（codegen/obfuscator.rs 已实测产出坏 IR；bytecode/obfuscator.rs 改程序语义）。
10. **bytecode 链路整体降级为实验性**：cay-bcgen 的 break/continue 错误代码生成、cay-run --obfuscate 空程序、jit.rs 非法 IR。

### 第三批（结构性，排期做）
11. **类型系统根基**：null 独立类型表示；比较运算符检查操作数；合并三套类型兼容表并统一参数顺序；重载试探用独立错误缓冲。
12. **parser/semantic 加递归深度上限**；生产路径 `unreachable!`/`panic!` 换 CayError 诊断。
13. **拆热点**：901/696/664/520 行函数；71 字段 IRGenerator。
14. **文档对齐**：AGENTS.md 全面更新（bak 文件、anyhow、bin 数量、CayError、CI 名、版本号）；CHANGELOG 补 6.2.0 条目。
15. **工程清理**：build.rs 去 `.git/index` 监听和 examples 复制、16 段版本块改循环；删 `src/main.rs`、`cay-pre.rs` 或补 [[bin]]；清根目录 temp/log/ELF 垃圾和 examples 的 35 个 .obj；取消 CI 的 `-A warnings` 或至少对 unreachable_pattern/dead_code 开警告。

---

## 十、统计附录

| 指标 | 数值 |
|---|---|
| src/ 总规模 | 84,751 行 / 159 个 .rs |
| 超 1000 行文件 | 24 个 |
| 生产代码 unwrap/expect | ~37 处（测试外） |
| unreachable!/panic!（生产路径） | ~7 处 |
| todo!/unimplemented! | 0 |
| unsafe | 33 处（集中 FFI，基本合理） |
| 注释掉的调试打印 | 53 处 |
| let _ = / .ok(); | 164 处（多数清理类，少数吞错） |
| println!/eprintln! | 996 处（大部分在 CLI bin） |
| 硬编码绝对路径 | 0 |
| 递归深度保护 | 仅 ir/inliner.rs 一处 |
