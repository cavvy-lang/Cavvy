import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';

/**
 * 代码补全提供器
 * 提供 Cavvy 语言的智能代码补全
 */
export class CavvyCompletionProvider implements vscode.CompletionItemProvider {

    // 关键字 - 更新到最新语法
    private keywords: string[] = [
        // 访问修饰符
        'public', 'private', 'protected',
        // 修饰符
        'static', 'final', 'abstract', 'native', 'Override',
        // 类型声明
        'class', 'interface', 'enum', 'extends', 'implements', 'namespace', 'using',
        // 基本类型
        'void', 'int', 'long', 'float', 'double', 'bool', 'boolean', 'char', 'string',
        // 现代类型声明
        'var', 'let', 'auto',
        // 控制流
        'if', 'else', 'while', 'for', 'do', 'switch', 'case', 'default', 'break', 'continue', 'return',
        // 异常处理
        'try', 'catch', 'finally', 'throw',
        // 其他关键字
        'new', 'null', 'true', 'false', 'this', 'super', 'instanceof', 'extern', 'scope', 'import', 'package'
    ];

    // FFI 类型
    private ffiTypes: string[] = [
        'c_int', 'c_long', 'c_short', 'c_char', 'c_byte',
        'c_float', 'c_double', 'c_bool', 'c_void',
        'size_t', 'ssize_t', 'uintptr_t', 'intptr_t',
        'uint8_t', 'uint16_t', 'uint32_t', 'uint64_t',
        'int8_t', 'int16_t', 'int32_t', 'int64_t'
    ];

    // 调用约定
    private callingConventions: string[] = [
        'cdecl', 'stdcall', 'fastcall', 'thiscall', 'vectorcall',
        'sysv64', 'win64', 'aapcs', 'msp430'
    ];

    // 预处理器指令 - 更新到最新语法
    private preprocessorDirectives: { name: string; detail: string; documentation: string; snippet?: string }[] = [
        {
            name: '#define',
            detail: '#define MACRO [value]',
            documentation: '定义一个宏。可以用于条件编译或简单的文本替换。\n\n示例：\n#define DEBUG\n#define VERSION "0.5.0.0"\n#define MAX_SIZE 100',
            snippet: '#define ${1:MACRO_NAME}${2: ${3:value}}'
        },
        {
            name: '#ifdef',
            detail: '#ifdef MACRO',
            documentation: '条件编译：如果宏已定义，则包含后续代码块。必须以 #endif 结束。\n\n示例：\n#ifdef DEBUG\n    println("Debug mode");\n#endif',
            snippet: '#ifdef ${1:MACRO_NAME}\n$2\n#endif'
        },
        {
            name: '#ifndef',
            detail: '#ifndef MACRO',
            documentation: '条件编译：如果宏未定义，则包含后续代码块。必须以 #endif 结束。\n\n示例：\n#ifndef RELEASE\n    println("Development mode");\n#endif',
            snippet: '#ifndef ${1:MACRO_NAME}\n$2\n#endif'
        },
        {
            name: '#if',
            detail: '#if expression',
            documentation: '条件编译：基于常量表达式。\n\n示例：\n#if VERSION > 100\n    // 版本大于 1.0.0\n#endif',
            snippet: '#if ${1:expression}\n$2\n#endif'
        },
        {
            name: '#elif',
            detail: '#elif expression',
            documentation: '条件编译：else if 分支。\n\n示例：\n#ifdef DEBUG\n    // 调试代码\n#elif defined(LOG_LEVEL)\n    // 日志代码\n#endif',
            snippet: '#elif ${1:expression}'
        },
        {
            name: '#else',
            detail: '#else',
            documentation: '条件编译：else 分支。\n\n示例：\n#ifdef DEBUG\n    // 调试代码\n#else\n    // 发布代码\n#endif'
        },
        {
            name: '#endif',
            detail: '#endif',
            documentation: '结束条件编译块。与 #ifdef、#ifndef、#if 配对使用。'
        },
        {
            name: '#undef',
            detail: '#undef MACRO',
            documentation: '取消定义一个宏。\n\n示例：\n#undef DEBUG',
            snippet: '#undef ${1:MACRO_NAME}'
        },
        {
            name: '#include',
            detail: '#include <header> or #include "header"',
            documentation: '包含头文件。\n\n示例：\n#include <stdio.h>\n#include "MyClass.cay"',
            snippet: '#include ${1:<${2:header}>}'
        }
    ];

    // 内置方法 - 更新到最新语法
    private builtinMethods: { name: string; detail: string; documentation: string }[] = [
        // 输出函数
        { name: 'print', detail: 'print(value: any) -> void', documentation: '打印值到控制台（不换行）' },
        { name: 'println', detail: 'println(value: any) -> void', documentation: '打印值到控制台并换行' },
        { name: 'printf', detail: 'printf(format: string, ...) -> void', documentation: '格式化输出，类似 C 语言 printf' },
        // 输入函数
        { name: 'readInt', detail: 'readInt() -> int', documentation: '从标准输入读取一个整数' },
        { name: 'readLong', detail: 'readLong() -> long', documentation: '从标准输入读取一个长整数' },
        { name: 'readFloat', detail: 'readFloat() -> float', documentation: '从标准输入读取一个浮点数' },
        { name: 'readDouble', detail: 'readDouble() -> double', documentation: '从标准输入读取一个双精度浮点数' },
        { name: 'readLine', detail: 'readLine() -> string', documentation: '从标准输入读取一行字符串' },
        { name: 'readChar', detail: 'readChar() -> char', documentation: '从标准输入读取一个字符' },
        // 类型转换函数
        { name: 'parseInt', detail: 'parseInt(s: string) -> int', documentation: '字符串转整数' },
        { name: 'parseLong', detail: 'parseLong(s: string) -> long', documentation: '字符串转长整数' },
        { name: 'parseFloat', detail: 'parseFloat(s: string) -> float', documentation: '字符串转浮点数' },
        { name: 'parseDouble', detail: 'parseDouble(s: string) -> double', documentation: '字符串转双精度浮点数' },
        // 字符串方法
        { name: 'length', detail: 'length() -> int', documentation: '获取字符串或数组的长度' },
        { name: 'charAt', detail: 'charAt(index: int) -> char', documentation: '获取字符串指定位置的字符' },
        { name: 'indexOf', detail: 'indexOf(str: string) -> int', documentation: '查找子字符串的位置' },
        { name: 'substring', detail: 'substring(start: int, end?: int) -> string', documentation: '获取子字符串' },
        { name: 'concat', detail: 'concat(str: string) -> string', documentation: '连接字符串' },
        { name: 'replace', detail: 'replace(old: string, new: string) -> string', documentation: '替换字符串' },
        { name: 'toLowerCase', detail: 'toLowerCase() -> string', documentation: '转换为小写' },
        { name: 'toUpperCase', detail: 'toUpperCase() -> string', documentation: '转换为大写' },
        { name: 'trim', detail: 'trim() -> string', documentation: '去除首尾空白' },
        { name: 'startsWith', detail: 'startsWith(prefix: string) -> bool', documentation: '是否以指定前缀开头' },
        { name: 'endsWith', detail: 'endsWith(suffix: string) -> bool', documentation: '是否以指定后缀结尾' },
        { name: 'contains', detail: 'contains(str: string) -> bool', documentation: '是否包含子串' },
        { name: 'equals', detail: 'equals(str: string) -> bool', documentation: '比较字符串相等' },
        { name: 'isEmpty', detail: 'isEmpty() -> bool', documentation: '是否为空字符串' },
        { name: 'compareTo', detail: 'compareTo(str: string) -> int', documentation: '字典序比较' },
        { name: 'valueOf', detail: 'valueOf(value: any) -> string', documentation: '数值转字符串（静态方法）' }
    ];

    // 代码片段 - 更新到最新语法
    private snippets: { name: string; snippet: string; detail: string; documentation?: string }[] = [
        {
            name: 'class',
            snippet: 'public class ${1:ClassName} {\n    public static void main() {\n        $2\n    }\n}',
            detail: '创建类',
            documentation: '创建一个带有 main 方法的公共类'
        },
        {
            name: 'main',
            snippet: 'public static void main() {\n    $1\n}',
            detail: '创建 main 方法',
            documentation: '创建程序入口点 main 方法'
        },
        {
            name: 'top-main',
            snippet: 'public int main() {\n    $1\n    return 0;\n}',
            detail: '顶层 main 函数',
            documentation: '创建顶层 main 函数（Cavvy 0.4.3+）'
        },
        {
            name: '@main',
            snippet: '@main\npublic class ${1:ClassName} {\n    public static void main() {\n        $2\n    }\n}',
            detail: '@main 注解类',
            documentation: '创建带有 @main 注解的类，指定程序入口'
        },
        {
            name: 'interface',
            snippet: 'public interface ${1:InterfaceName} {\n    ${2:void} ${3:methodName}(${4:params});\n}',
            detail: '创建接口',
            documentation: '创建接口定义'
        },
        {
            name: 'for',
            snippet: 'for (int ${1:i} = 0; ${1:i} < ${2:count}; ${1:i}++) {\n    $3\n}',
            detail: 'for 循环',
            documentation: '标准 for 循环结构'
        },
        {
            name: 'fori',
            snippet: 'for (int ${1:i} = ${2:0}; ${1:i} < ${3:array}.length; ${1:i}++) {\n    $4\n}',
            detail: '数组遍历 for 循环',
            documentation: '遍历数组的标准 for 循环'
        },
        {
            name: 'foreach',
            snippet: 'for (${1:Type} ${2:item} : ${3:collection}) {\n    $4\n}',
            detail: '增强 for 循环',
            documentation: '遍历集合的增强 for 循环'
        },
        {
            name: 'while',
            snippet: 'while (${1:condition}) {\n    $2\n}',
            detail: 'while 循环',
            documentation: 'while 循环结构'
        },
        {
            name: 'dowhile',
            snippet: 'do {\n    $2\n} while (${1:condition});',
            detail: 'do-while 循环',
            documentation: '至少执行一次的 do-while 循环'
        },
        {
            name: 'if',
            snippet: 'if (${1:condition}) {\n    $2\n}',
            detail: 'if 语句',
            documentation: '条件判断语句'
        },
        {
            name: 'ifelse',
            snippet: 'if (${1:condition}) {\n    $2\n} else {\n    $3\n}',
            detail: 'if-else 语句',
            documentation: '条件判断与备选分支'
        },
        {
            name: 'switch',
            snippet: 'switch (${1:value}) {\n    case ${2:1}:\n        $3\n        break;\n    default:\n        break;\n}',
            detail: 'switch 语句',
            documentation: '多分支选择语句'
        },
        {
            name: 'method',
            snippet: '${1:public} ${2:static} ${3:void} ${4:methodName}(${5:params}) {\n    $6\n}',
            detail: '创建方法',
            documentation: '创建类方法'
        },
        {
            name: 'method-varargs',
            snippet: '${1:public} ${2:static} ${3:int} ${4:methodName}(${3:int}... ${5:args}) {\n    $6\n}',
            detail: '可变参数方法',
            documentation: '创建接受可变数量参数的方法'
        },
        {
            name: 'lambda',
            snippet: '(${1:params}) -> ${2:expression}',
            detail: 'Lambda 表达式',
            documentation: '创建 Lambda 表达式'
        },
        {
            name: 'lambda-block',
            snippet: '(${1:params}) -> {\n    $2\n}',
            detail: 'Lambda 表达式（代码块）',
            documentation: '创建带代码块的 Lambda 表达式'
        },
        {
            name: 'method-ref',
            snippet: '${1:ClassName}::${2:methodName}',
            detail: '方法引用',
            documentation: '创建方法引用'
        },
        {
            name: 'println',
            snippet: 'println(${1:message});',
            detail: '打印并换行',
            documentation: '输出内容到控制台并换行'
        },
        {
            name: 'print',
            snippet: 'print(${1:message});',
            detail: '打印',
            documentation: '输出内容到控制台不换行'
        },
        {
            name: 'newarray',
            snippet: '${1:int}[] ${2:arr} = new ${1:int}[${3:size}];',
            detail: '创建一维数组',
            documentation: '创建指定类型和大小的数组'
        },
        {
            name: 'newarray2d',
            snippet: '${1:int}[][] ${2:matrix} = new ${1:int}[${3:rows}][${4:cols}];',
            detail: '创建二维数组',
            documentation: '创建二维矩阵数组'
        },
        {
            name: 'array-init',
            snippet: '${1:int}[] ${2:arr} = {${3:1, 2, 3}};',
            detail: '数组初始化',
            documentation: '使用初始化列表创建数组'
        },
        {
            name: 'ifdef',
            snippet: '#ifdef ${1:DEBUG}\n$2\n#endif',
            detail: '条件编译 #ifdef',
            documentation: '如果宏已定义则编译代码块'
        },
        {
            name: 'ifndef',
            snippet: '#ifndef ${1:RELEASE}\n$2\n#endif',
            detail: '条件编译 #ifndef',
            documentation: '如果宏未定义则编译代码块'
        },
        {
            name: 'define',
            snippet: '#define ${1:MACRO_NAME}',
            detail: '定义宏',
            documentation: '定义预处理器宏'
        },
        {
            name: 'final-var',
            snippet: 'final ${1:int} ${2:CONST_NAME} = ${3:value};',
            detail: 'final 常量',
            documentation: '定义不可修改的常量'
        },
        {
            name: 'static-field',
            snippet: 'static ${1:int} ${2:fieldName}${3: = ${4:initialValue}};',
            detail: '静态字段',
            documentation: '定义类级别的静态字段'
        },
        {
            name: 'var-decl',
            snippet: 'var ${1:name}: ${2:int} = ${3:value};',
            detail: 'var 变量声明',
            documentation: '使用 var 声明变量（类型后置）'
        },
        {
            name: 'auto-decl',
            snippet: 'auto ${1:name} = ${2:value};',
            detail: 'auto 自动推断',
            documentation: '使用 auto 自动推断类型'
        },
        {
            name: 'cast',
            snippet: '(${1:int})${2:expression}',
            detail: '类型转换',
            documentation: '显式类型转换'
        },
        {
            name: 'extern',
            snippet: 'extern {\n    ${1:c_int} ${2:funcName}(${3:c_int} ${4:param});\n}',
            detail: '外部函数声明',
            documentation: '声明外部 C 函数'
        },
        {
            name: 'extern-stdcall',
            snippet: 'extern stdcall {\n    ${1:c_int} ${2:funcName}(${3:c_int} ${4:param});\n}',
            detail: '外部函数声明 (stdcall)',
            documentation: '声明使用 stdcall 调用约定的外部函数'
        },
        {
            name: 'scope',
            snippet: 'scope {\n    $1\n}',
            detail: 'scope 块',
            documentation: '创建栈作用域块（Cavvy 0.5.0+）'
        },
        {
            name: 'class-extends',
            snippet: 'public class ${1:Child} extends ${2:Parent} {\n    @Override\n    public ${3:void} ${4:methodName}() {\n        super.${4:methodName}();\n        $5\n    }\n}',
            detail: '继承类',
            documentation: '创建继承父类的子类'
        },
        {
            name: 'class-implements',
            snippet: 'public class ${1:ClassName} implements ${2:InterfaceName} {\n    @Override\n    public ${3:void} ${4:methodName}() {\n        $5\n    }\n}',
            detail: '实现接口',
            documentation: '创建实现接口的类'
        },
        {
            name: 'constructor',
            snippet: 'public ${1:ClassName}(${2:params}) {\n    $3\n}',
            detail: '构造函数',
            documentation: '创建构造函数'
        },
        {
            name: 'constructor-chain',
            snippet: 'public ${1:ClassName}(${2:params}) {\n    super(${3:args});\n    $4\n}',
            detail: '构造函数（调用父类）',
            documentation: '创建调用父类构造函数的构造函数'
        },
        {
            name: 'static-block',
            snippet: 'static {\n    $1\n}',
            detail: '静态初始化块',
            documentation: '创建静态初始化块'
        }
    ];

    /**
     * 提供代码补全项
     */
    provideCompletionItems(
        document: vscode.TextDocument,
        position: vscode.Position,
        _token: vscode.CancellationToken,
        _context: vscode.CompletionContext
    ): vscode.ProviderResult<vscode.CompletionItem[] | vscode.CompletionList> {

        const completions: vscode.CompletionItem[] = [];
        const lineText = document.lineAt(position).text.substring(0, position.character);

        // 检查是否在行首（可能输入预处理器指令）
        const isLineStart = /^\s*$/.test(lineText);
        const isPreprocessor = /^\s*#/.test(lineText);

        // 如果在行首或已输入 #，添加预处理器指令
        if (isLineStart || isPreprocessor) {
            this.preprocessorDirectives.forEach(directive => {
                const item = new vscode.CompletionItem(directive.name, vscode.CompletionItemKind.Keyword);
                item.detail = directive.detail;
                item.documentation = new vscode.MarkdownString(directive.documentation);
                if (directive.snippet) {
                    item.insertText = new vscode.SnippetString(directive.snippet);
                }
                item.sortText = '0' + directive.name; // 让预处理器指令排在前面
                completions.push(item);
            });

            // 检查是否在 #include 语句中，提供文件路径补全
            const includeMatch = lineText.match(/^\s*#include\s+["<]([^">]*)$/);
            if (includeMatch) {
                const fileCompletions = this.getIncludeFileCompletions(document, includeMatch[1]);
                completions.push(...fileCompletions);
            }
        }

        // 检查是否在 extern 块内
        const isInExtern = this.isInExternBlock(document, position);
        if (isInExtern) {
            // 添加 FFI 类型
            this.ffiTypes.forEach(type => {
                const item = new vscode.CompletionItem(type, vscode.CompletionItemKind.TypeParameter);
                item.detail = 'FFI 类型';
                item.documentation = new vscode.MarkdownString(`Cavvy FFI 类型: ${type}`);
                completions.push(item);
            });

            // 添加调用约定
            this.callingConventions.forEach(conv => {
                const item = new vscode.CompletionItem(conv, vscode.CompletionItemKind.Keyword);
                item.detail = '调用约定';
                completions.push(item);
            });
        }

        // 注意：关键字、内置方法和代码片段现在由 LSP 服务器提供
        // 这里只提供扩展端特有的补全（如预处理器指令、文件路径等）

        // 从文档中提取用户定义的符号
        const userSymbols = this.extractUserDefinedSymbols(document);
        userSymbols.forEach(symbol => {
            let kind: vscode.CompletionItemKind;
            switch (symbol.type) {
                case 'class':
                    kind = vscode.CompletionItemKind.Class;
                    break;
                case 'interface':
                    kind = vscode.CompletionItemKind.Interface;
                    break;
                case 'method':
                    kind = vscode.CompletionItemKind.Method;
                    break;
                case 'variable':
                    kind = vscode.CompletionItemKind.Variable;
                    break;
                default:
                    kind = vscode.CompletionItemKind.Text;
            }
            const item = new vscode.CompletionItem(symbol.name, kind);
            item.detail = `用户定义的${symbol.type}`;
            completions.push(item);
        });

        return completions;
    }

    /**
     * 检查是否在 extern 块内
     */
    private isInExternBlock(document: vscode.TextDocument, position: vscode.Position): boolean {
        const text = document.getText(new vscode.Range(new vscode.Position(0, 0), position));
        // 简单检查：查找最近的 extern { 和对应的 }
        const lastExtern = text.lastIndexOf('extern');
        if (lastExtern === -1) {
            return false;
        }
        const lastOpenBrace = text.indexOf('{', lastExtern);
        if (lastOpenBrace === -1) {
            return false;
        }
        // 检查是否有闭合的 }
        const afterOpenBrace = text.substring(lastOpenBrace + 1);
        const closeBraceIndex = afterOpenBrace.indexOf('}');
        return closeBraceIndex === -1; // 如果没有闭合的 }，则在 extern 块内
    }

    /**
     * 提取用户定义的符号
     */
    private extractUserDefinedSymbols(document: vscode.TextDocument): Array<{ name: string; type: string }> {
        const symbols: Array<{ name: string; type: string }> = [];
        const text = document.getText();

        // 提取类名
        const classMatches = text.match(/class\s+([a-zA-Z_][a-zA-Z0-9_]*)/g);
        if (classMatches) {
            classMatches.forEach(match => {
                const name = match.replace(/class\s+/, '');
                if (!symbols.some(s => s.name === name)) {
                    symbols.push({ name, type: 'class' });
                }
            });
        }

        // 提取接口名
        const interfaceMatches = text.match(/interface\s+([a-zA-Z_][a-zA-Z0-9_]*)/g);
        if (interfaceMatches) {
            interfaceMatches.forEach(match => {
                const name = match.replace(/interface\s+/, '');
                if (!symbols.some(s => s.name === name)) {
                    symbols.push({ name, type: 'interface' });
                }
            });
        }

        // 提取方法名
        const methodPattern = /\b(?:public|private|protected)?\s*(?:static)?\s*(?:final)?\s*(?:abstract)?\s*(?:int|long|float|double|bool|string|char|void|auto)\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\(/g;
        let methodMatch;
        while ((methodMatch = methodPattern.exec(text)) !== null) {
            const name = methodMatch[1];
            if (!symbols.some(s => s.name === name)) {
                symbols.push({ name, type: 'method' });
            }
        }

        // 提取变量名（包括 var/let/auto 声明）
        const varPattern = /\b(?:final\s+)?(?:var|let|auto)\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*(?::|=|;)/g;
        let varMatch;
        while ((varMatch = varPattern.exec(text)) !== null) {
            const name = varMatch[1];
            if (!symbols.some(s => s.name === name)) {
                symbols.push({ name, type: 'variable' });
            }
        }

        // 提取传统变量声明
        const traditionalVarPattern = /\b(int|long|float|double|bool|string|char)\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*(?:=|;|\[)/g;
        let tradVarMatch;
        while ((tradVarMatch = traditionalVarPattern.exec(text)) !== null) {
            const name = tradVarMatch[2];
            if (!symbols.some(s => s.name === name)) {
                symbols.push({ name, type: 'variable' });
            }
        }

        return symbols;
    }

    /**
     * 获取 #include 的文件路径补全
     * @param document 当前文档
     * @param partialPath 已输入的部分路径
     * @returns 补全项数组
     */
    private getIncludeFileCompletions(document: vscode.TextDocument, partialPath: string): vscode.CompletionItem[] {
        const completions: vscode.CompletionItem[] = [];
        const documentDir = path.dirname(document.fileName);

        // 搜索路径列表
        const searchPaths: string[] = [
            documentDir,
            path.join(documentDir, 'caylibs'),
            path.join(documentDir, '..', 'caylibs'),
        ];

        // 如果工作区有 caylibs 目录，也加入搜索路径
        const workspaceFolders = vscode.workspace.workspaceFolders;
        if (workspaceFolders) {
            for (const folder of workspaceFolders) {
                const caylibsPath = path.join(folder.uri.fsPath, 'caylibs');
                if (fs.existsSync(caylibsPath) && !searchPaths.includes(caylibsPath)) {
                    searchPaths.push(caylibsPath);
                }
            }
        }

        // 确定搜索目录和文件名前缀
        let searchDir: string;
        let filePrefix: string;

        if (partialPath.includes('/') || partialPath.includes('\\')) {
            // 路径包含目录分隔符
            const lastSepIndex = Math.max(partialPath.lastIndexOf('/'), partialPath.lastIndexOf('\\'));
            const dirPart = partialPath.substring(0, lastSepIndex);
            filePrefix = partialPath.substring(lastSepIndex + 1);

            // 尝试相对当前文件目录
            searchDir = path.join(documentDir, dirPart);
            if (!fs.existsSync(searchDir)) {
                // 尝试在每个搜索路径下查找
                for (const basePath of searchPaths) {
                    const tryPath = path.join(basePath, dirPart);
                    if (fs.existsSync(tryPath)) {
                        searchDir = tryPath;
                        break;
                    }
                }
            }
        } else {
            // 只有文件名前缀，没有目录
            searchDir = documentDir;
            filePrefix = partialPath;
        }

        // 如果搜索目录存在，列出匹配的文件
        if (fs.existsSync(searchDir) && fs.statSync(searchDir).isDirectory()) {
            try {
                const entries = fs.readdirSync(searchDir, { withFileTypes: true });

                for (const entry of entries) {
                    const entryName = entry.name;

                    // 检查是否匹配前缀
                    if (!entryName.toLowerCase().startsWith(filePrefix.toLowerCase())) {
                        continue;
                    }

                    if (entry.isDirectory()) {
                        // 添加目录项
                        const item = new vscode.CompletionItem(entryName + '/', vscode.CompletionItemKind.Folder);
                        item.detail = '目录';
                        item.insertText = entryName + '/';
                        item.command = { command: 'editor.action.triggerSuggest', title: '继续补全' };
                        completions.push(item);
                    } else if (entryName.endsWith('.cay') || entryName.endsWith('.h') || entryName.endsWith('.c')) {
                        // 添加文件项
                        const item = new vscode.CompletionItem(entryName, vscode.CompletionItemKind.File);
                        item.detail = '头文件';
                        item.insertText = entryName;
                        completions.push(item);
                    }
                }
            } catch (error) {
                // 忽略读取目录错误
            }
        }

        return completions;
    }
}
