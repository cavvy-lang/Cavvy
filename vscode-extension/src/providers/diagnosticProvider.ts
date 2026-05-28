import * as vscode from 'vscode';
import { exec } from 'child_process';
import { promisify } from 'util';
import * as path from 'path';

const execAsync = promisify(exec);

/**
 * 诊断提供器
 * 用于检测 Cavvy 代码中的语法错误
 */
export class CavvyDiagnosticProvider {

    private diagnosticCollection: vscode.DiagnosticCollection;
    private disposables: vscode.Disposable[] = [];
    // 每个文档独立的定时器，避免快速输入时的冲突
    private documentTimeouts: Map<string, NodeJS.Timeout> = new Map();
    private config: vscode.WorkspaceConfiguration;
    private outputChannel: vscode.OutputChannel;
    // 跟踪每个文档的最后诊断时间，用于清理陈旧的诊断
    private lastDiagnosticTime: Map<string, number> = new Map();
    // 定期清理间隔（毫秒）
    private readonly CLEANUP_INTERVAL = 30000; // 30秒
    private cleanupTimer: NodeJS.Timeout | undefined;
    // 记录每个文档的版本号，避免旧检查覆盖新结果
    private documentVersions: Map<string, number> = new Map();

    constructor() {
        this.diagnosticCollection = vscode.languages.createDiagnosticCollection('cavvy');
        this.config = vscode.workspace.getConfiguration('cavvyAnalyzer');
        this.outputChannel = vscode.window.createOutputChannel('Cavvy Diagnostics');
    }

    /**
     * 激活诊断提供器
     * @param context 插件上下文
     */
    activate(context: vscode.ExtensionContext): void {
        // 监听文档打开事件
        const onDidOpenDisposable = vscode.workspace.onDidOpenTextDocument(
            (document) => this.onDocumentOpen(document)
        );
        context.subscriptions.push(onDidOpenDisposable);
        this.disposables.push(onDidOpenDisposable);

        // 监听文档内容变更事件
        const onDidChangeDisposable = vscode.workspace.onDidChangeTextDocument(
            (event) => this.onDocumentChange(event)
        );
        context.subscriptions.push(onDidChangeDisposable);
        this.disposables.push(onDidChangeDisposable);

        // 监听文档保存事件
        const onDidSaveDisposable = vscode.workspace.onDidSaveTextDocument(
            (document) => this.onDocumentSave(document)
        );
        context.subscriptions.push(onDidSaveDisposable);
        this.disposables.push(onDidSaveDisposable);

        // 监听文档关闭事件 - 清除该文档的诊断
        const onDidCloseDisposable = vscode.workspace.onDidCloseTextDocument(
            (document) => this.onDocumentClose(document)
        );
        context.subscriptions.push(onDidCloseDisposable);
        this.disposables.push(onDidCloseDisposable);

        // 初始化时检查所有已打开的文档
        vscode.workspace.textDocuments.forEach((doc) => {
            if (this.isCavvyFile(doc)) {
                this.scheduleCheck(doc);
            }
        });

        // 启动定期清理定时器
        this.startCleanupTimer();

        this.log('诊断提供器已激活');
    }

    /**
     * 启动定期清理定时器
     */
    private startCleanupTimer(): void {
        this.cleanupTimer = setInterval(() => {
            this.cleanupStaleDiagnostics();
        }, this.CLEANUP_INTERVAL);
    }

    /**
     * 清理陈旧的诊断信息
     */
    private cleanupStaleDiagnostics(): void {
        const now = Date.now();
        const staleThreshold = this.CLEANUP_INTERVAL * 2; // 60秒

        for (const [uri, lastTime] of this.lastDiagnosticTime.entries()) {
            if (now - lastTime > staleThreshold) {
                // 清除陈旧的诊断
                const uriObj = vscode.Uri.parse(uri);
                this.diagnosticCollection.delete(uriObj);
                this.lastDiagnosticTime.delete(uri);
                this.log(`清理陈旧诊断: ${uri}`);
            }
        }
    }

    /**
     * 更新文档的诊断时间戳
     */
    private updateDiagnosticTimestamp(document: vscode.TextDocument): void {
        this.lastDiagnosticTime.set(document.uri.toString(), Date.now());
    }

    /**
     * 检查文档是否是 Cavvy 文件
     * @param document 文档
     * @returns 是否是 Cavvy 文件
     */
    private isCavvyFile(document: vscode.TextDocument): boolean {
        // IR 文件 (.ll) 不应该进行 Cavvy 语法检查
        if (document.fileName.endsWith('.ll')) {
            return false;
        }
        return document.languageId === 'cavvy' ||
               document.fileName.endsWith('.cay') ||
               document.fileName.endsWith('.eol') ||
               document.fileName.endsWith('.caybc');
    }

    /**
     * 文档打开时的处理
     * @param document 文档
     */
    private onDocumentOpen(document: vscode.TextDocument): void {
        if (this.isCavvyFile(document)) {
            this.scheduleCheck(document);
        }
    }

    /**
     * 文档内容变更时的处理
     * @param event 文本文档变更事件
     */
    private onDocumentChange(event: vscode.TextDocumentChangeEvent): void {
        if (this.isCavvyFile(event.document)) {
            this.scheduleCheck(event.document);
        }
    }

    /**
     * 文档保存时的处理
     * @param document 文档
     */
    private onDocumentSave(document: vscode.TextDocument): void {
        if (this.isCavvyFile(document)) {
            this.checkDocument(document);
        }
    }

    /**
     * 文档关闭时的处理 - 清除诊断
     * @param document 文档
     */
    private onDocumentClose(document: vscode.TextDocument): void {
        this.diagnosticCollection.delete(document.uri);
        this.log(`清除已关闭文档的诊断: ${document.fileName}`);
    }

    /**
     * 清除所有诊断
     */
    clearAllDiagnostics(): void {
        this.diagnosticCollection.clear();
        this.log('清除所有诊断');
    }

    /**
     * 清除指定文档的诊断
     * @param document 文档
     */
    clearDocumentDiagnostics(document: vscode.TextDocument): void {
        if (this.isCavvyFile(document)) {
            this.diagnosticCollection.delete(document.uri);
            this.log(`清除文档诊断: ${document.fileName}`);
        }
    }

    /**
     * 调度检查（带延迟）
     * @param document 文档
     */
    private scheduleCheck(document: vscode.TextDocument): void {
        if (!this.config.get<boolean>('enableDiagnostics', true)) {
            return;
        }

        const uri = document.uri.toString();
        const currentVersion = document.version;

        // 记录文档版本号
        this.documentVersions.set(uri, currentVersion);

        // 清除该文档之前的定时器
        const existingTimeout = this.documentTimeouts.get(uri);
        if (existingTimeout) {
            clearTimeout(existingTimeout);
        }

        // 立即清除该文档的旧诊断，避免错误残留
        this.diagnosticCollection.delete(document.uri);

        // 设置新的定时器
        const delay = this.config.get<number>('diagnosticDelay', 300);
        const timeout = setTimeout(() => {
            // 检查文档版本是否已变更，避免旧检查覆盖新结果
            const latestVersion = this.documentVersions.get(uri);
            if (latestVersion !== undefined && latestVersion > currentVersion) {
                this.log(`跳过过时的检查: ${document.fileName} (版本 ${currentVersion} < ${latestVersion})`);
                return;
            }
            this.checkDocument(document);
            this.documentTimeouts.delete(uri);
        }, delay);

        this.documentTimeouts.set(uri, timeout);
    }

    /**
     * 检查文档语法
     * @param document 文档
     */
    async checkDocument(document: vscode.TextDocument): Promise<void> {
        if (!this.config.get<boolean>('enableDiagnostics', true)) {
            return;
        }

        const uri = document.uri.toString();
        const checkVersion = document.version;

        // 再次检查版本，确保不会用旧结果覆盖新结果
        const latestVersion = this.documentVersions.get(uri);
        if (latestVersion !== undefined && latestVersion > checkVersion) {
            this.log(`跳过过时的文档检查: ${document.fileName} (版本 ${checkVersion} < ${latestVersion})`);
            return;
        }

        this.log(`检查文档: ${document.fileName} (版本 ${checkVersion})`);
        const diagnostics: vscode.Diagnostic[] = [];

        // 检查是否只使用 LSP 诊断（默认 true）
        const useLspOnly = this.config.get<boolean>('useLspDiagnosticsOnly', true);
        
        if (useLspOnly) {
            // 只使用 LSP 诊断，跳过内置检查
            this.log('使用 LSP 诊断模式，跳过内置语法检查');
            // 清除之前的诊断，让 LSP 接管
            this.diagnosticCollection.delete(document.uri);
            return;
        }

        try {
            // 首先进行基本的语法检查
            const basicDiagnostics = this.performBasicSyntaxCheck(document);
            diagnostics.push(...basicDiagnostics);

            // 如果配置了检查器路径，尝试使用检查器进行更详细的检查
            const checkerPath = this.config.get<string>('checkerPath', 'cay-check');
            if (checkerPath && checkerPath !== '') {
                try {
                    const checkerDiagnostics = await this.runChecker(document, checkerPath);
                    diagnostics.push(...checkerDiagnostics);
                } catch (error) {
                    this.log(`检查器运行失败: ${error}`);
                }
            }

            this.log(`发现 ${diagnostics.length} 个问题`);
        } catch (error) {
            this.log(`语法检查出错: ${error}`);
            console.error('Cavvy 语法检查出错:', error);
        }

        // 最终版本检查，确保异步操作期间文档没有被修改
        const finalVersion = this.documentVersions.get(uri);
        if (finalVersion !== undefined && finalVersion > checkVersion) {
            this.log(`放弃过时的检查结果: ${document.fileName} (版本 ${checkVersion} < ${finalVersion})`);
            return;
        }

        // 设置诊断前清除旧的诊断
        this.diagnosticCollection.delete(document.uri);
        this.diagnosticCollection.set(document.uri, diagnostics);

        // 更新诊断时间戳
        this.updateDiagnosticTimestamp(document);
    }

    /**
     * 执行基本语法检查
     * @param document 文档
     * @returns 诊断数组
     */
    private performBasicSyntaxCheck(document: vscode.TextDocument): vscode.Diagnostic[] {
        const diagnostics: vscode.Diagnostic[] = [];
        const text = document.getText();
        const lines = text.split('\n');

        let inBlockComment = false;
        const braceStack: { char: string; line: number; col: number }[] = [];

        // 重置上下文
        this.currentContext = {
            inClass: false,
            inMethod: false,
            className: null,
            methodName: null,
            hasMainMethod: false,
            braceDepth: 0,
            loopStack: [],
            returnLines: new Set()
        };

        for (let i = 0; i < lines.length; i++) {
            const line = lines[i];

            // 处理块注释
            if (inBlockComment) {
                const endIndex = line.indexOf('*/');
                if (endIndex !== -1) {
                    inBlockComment = false;
                }
                continue;
            }

            // 检查块注释开始
            const blockCommentStart = line.indexOf('/*');
            if (blockCommentStart !== -1) {
                const blockCommentEnd = line.indexOf('*/', blockCommentStart + 2);
                if (blockCommentEnd === -1) {
                    inBlockComment = true;
                }
                continue;
            }

            // 跳过纯注释行
            const trimmedLine = line.trim();
            if (trimmedLine.startsWith('//')) {
                continue;
            }

            // 检查括号匹配
            for (let j = 0; j < line.length; j++) {
                const char = line[j];

                // 跳过字符串内的字符
                if (char === '"' || char === "'") {
                    j++;
                    while (j < line.length && line[j] !== char) {
                        if (line[j] === '\\') j++;
                        j++;
                    }
                    continue;
                }

                // 跳过行注释
                if (char === '/' && j + 1 < line.length && line[j + 1] === '/') {
                    break;
                }

                if (char === '{' || char === '(' || char === '[') {
                    braceStack.push({ char, line: i, col: j });
                    if (char === '{') {
                        this.currentContext.braceDepth++;
                    }
                } else if (char === '}' || char === ')' || char === ']') {
                    const expectedOpen = char === '}' ? '{' : (char === ')' ? '(' : '[');
                    if (braceStack.length === 0 || braceStack[braceStack.length - 1].char !== expectedOpen) {
                        const range = new vscode.Range(i, j, i, j + 1);
                        const diagnostic = new vscode.Diagnostic(
                            range,
                            `不匹配的括号: 期望 '${expectedOpen}' 但找到 '${char}'`,
                            vscode.DiagnosticSeverity.Error
                        );
                        diagnostic.code = 'unmatched-brace';
                        diagnostics.push(diagnostic);
                    } else {
                        braceStack.pop();
                        if (char === '}') {
                            this.currentContext.braceDepth--;
                        }
                    }
                }
            }

            // 检查基本语法错误
            const lineDiagnostics = this.checkLineSyntax(document, line, i);
            diagnostics.push(...lineDiagnostics);
        }

        // 检查未闭合的括号
        for (const unclosed of braceStack) {
            const range = new vscode.Range(unclosed.line, unclosed.col, unclosed.line, unclosed.col + 1);
            const matching = unclosed.char === '{' ? '}' : (unclosed.char === '(' ? ')' : ']');
            const diagnostic = new vscode.Diagnostic(
                range,
                `未闭合的括号: '${unclosed.char}' 没有匹配的 '${matching}'`,
                vscode.DiagnosticSeverity.Error
            );
            diagnostic.code = 'unclosed-brace';
            diagnostics.push(diagnostic);
        }

        return diagnostics;
    }

    // 追踪当前上下文
    private currentContext: {
        inClass: boolean;
        inMethod: boolean;
        className: string | null;
        methodName: string | null;
        hasMainMethod: boolean;
        braceDepth: number;
        loopStack: string[];
        returnLines: Set<number>;
    } = {
        inClass: false,
        inMethod: false,
        className: null,
        methodName: null,
        hasMainMethod: false,
        braceDepth: 0,
        loopStack: [],
        returnLines: new Set()
    };

    /**
     * 检查单行语法
     * @param document 文档
     * @param line 行内容
     * @param lineNum 行号
     * @returns 诊断数组
     */
    private checkLineSyntax(
        document: vscode.TextDocument,
        line: string,
        lineNum: number
    ): vscode.Diagnostic[] {
        const diagnostics: vscode.Diagnostic[] = [];
        const trimmedLine = line.trim();

        // 跳过空行和纯注释行
        if (trimmedLine.length === 0 || trimmedLine.startsWith('//')) {
            return diagnostics;
        }

        // 检查类声明
        const classMatch = trimmedLine.match(/^(?:public\s+|abstract\s+|final\s+)*class\s+(\w+)/);
        if (classMatch) {
            this.currentContext.inClass = true;
            this.currentContext.className = classMatch[1];
            this.currentContext.hasMainMethod = false;

            // 检查类名是否符合 PascalCase
            const className = classMatch[1];
            if (!/^[A-Z]/.test(className)) {
                const startIdx = line.indexOf(className);
                const range = new vscode.Range(lineNum, startIdx, lineNum, startIdx + className.length);
                const diagnostic = new vscode.Diagnostic(
                    range,
                    `类名 '${className}' 应该使用 PascalCase（首字母大写）`,
                    vscode.DiagnosticSeverity.Warning
                );
                diagnostic.code = 'class-naming-convention';
                diagnostics.push(diagnostic);
            }

            // 检查是否有 @main 注解
            if (lineNum > 0) {
                const prevLine = document.lineAt(lineNum - 1).text.trim();
                if (prevLine === '@main') {
                    this.currentContext.hasMainMethod = true;
                }
            }
        }

        // 检查方法声明
        const methodMatch = trimmedLine.match(
            /^(?:public|private|protected)?\s*(?:static|final|abstract|native)?\s*(?:int|long|float|double|bool|string|char|void|\w+)\s+(\w+)\s*\(/
        );
        if (methodMatch) {
            const methodName = methodMatch[1];
            const isInClass = this.currentContext.inClass;

            // 只有在类内部或者是顶层 main 方法时才处理
            if (isInClass || methodName === 'main') {
                this.currentContext.inMethod = true;
                this.currentContext.methodName = methodName;
                this.currentContext.returnLines.clear();

                // 检查 main 方法
                if (methodName === 'main') {
                    this.currentContext.hasMainMethod = true;

                    // 只有在类内部的 main 方法才需要 static
                    // Cavvy 支持顶层 main 函数，不需要 static
                    if (isInClass && !trimmedLine.includes('static')) {
                        const startIdx = line.indexOf('main');
                        const range = new vscode.Range(lineNum, startIdx, lineNum, startIdx + 4);
                        const diagnostic = new vscode.Diagnostic(
                            range,
                            "类中的 main 方法应该是 static 的",
                            vscode.DiagnosticSeverity.Error
                        );
                        diagnostic.code = 'main-not-static';
                        diagnostics.push(diagnostic);
                    }

                    // 检查 main 方法的返回类型 - Cavvy 支持 int 或 void
                    const returnTypeMatch = trimmedLine.match(/^(?:public|private|protected)?\s*(?:static|final|abstract|native)?\s*(\w+)\s+main/);
                    if (returnTypeMatch) {
                        const returnType = returnTypeMatch[1];
                        if (returnType !== 'void' && returnType !== 'int') {
                            const startIdx = line.indexOf(returnType);
                            const range = new vscode.Range(lineNum, startIdx, lineNum, startIdx + returnType.length);
                            const diagnostic = new vscode.Diagnostic(
                                range,
                                "main 方法应该返回 void 或 int",
                                vscode.DiagnosticSeverity.Error
                            );
                            diagnostic.code = 'main-return-type';
                            diagnostics.push(diagnostic);
                        }
                    }
                }

                // 检查方法名是否符合 camelCase（main 方法除外）
                if (!/^[a-z]/.test(methodName) && methodName !== 'main') {
                    const startIdx = line.indexOf(methodName);
                    const range = new vscode.Range(lineNum, startIdx, lineNum, startIdx + methodName.length);
                    const diagnostic = new vscode.Diagnostic(
                        range,
                        `方法名 '${methodName}' 应该使用 camelCase（首字母小写）`,
                        vscode.DiagnosticSeverity.Warning
                    );
                    diagnostic.code = 'method-naming-convention';
                    diagnostics.push(diagnostic);
                }
            }
        }

        // 检查变量声明
        const varMatch = trimmedLine.match(
            /^(?:int|long|float|double|bool|string|char|\w+)\s+(\w+)\s*=/
        );
        if (varMatch && this.currentContext.inMethod) {
            const varName = varMatch[1];

            // 检查变量名是否符合 camelCase
            if (!/^[a-z]/.test(varName)) {
                const startIdx = line.indexOf(varName);
                const range = new vscode.Range(lineNum, startIdx, lineNum, startIdx + varName.length);
                const diagnostic = new vscode.Diagnostic(
                    range,
                    `变量名 '${varName}' 应该使用 camelCase（首字母小写）`,
                    vscode.DiagnosticSeverity.Warning
                );
                diagnostic.code = 'variable-naming-convention';
                diagnostics.push(diagnostic);
            }
        }

        // 检查循环结构
        if (trimmedLine.startsWith('for ') || trimmedLine.startsWith('while ') || trimmedLine.startsWith('do ')) {
            this.currentContext.loopStack.push(trimmedLine.split(' ')[0]);
        }

        // 检查 break/continue
        if ((trimmedLine.startsWith('break') || trimmedLine.startsWith('continue')) &&
            this.currentContext.loopStack.length === 0) {
            const keyword = trimmedLine.split(' ')[0];
            const range = new vscode.Range(lineNum, 0, lineNum, keyword.length);
            const diagnostic = new vscode.Diagnostic(
                range,
                `'${keyword}' 只能在循环内部使用`,
                vscode.DiagnosticSeverity.Error
            );
            diagnostic.code = 'break-outside-loop';
            diagnostics.push(diagnostic);
        }

        // 检查 return 语句
        if (trimmedLine.startsWith('return')) {
            this.currentContext.returnLines.add(lineNum);

            // 检查 void 方法是否返回值
            if (this.currentContext.methodName &&
                trimmedLine.match(/return\s+\w+/) &&
                this.currentContext.methodName !== 'main') {
                // 这里简化处理，实际需要检查方法返回类型
            }
        }

        // 检查未使用的变量（简化检查）
        const unusedVarMatch = trimmedLine.match(/^(?:int|long|float|double|bool|string|char)\s+(\w+)\s*;?$/);
        if (unusedVarMatch) {
            const varName = unusedVarMatch[1];
            const range = new vscode.Range(lineNum, line.indexOf(varName), lineNum, line.indexOf(varName) + varName.length);
            const diagnostic = new vscode.Diagnostic(
                range,
                `变量 '${varName}' 可能未使用`,
                vscode.DiagnosticSeverity.Information
            );
            diagnostic.code = 'unused-variable';
            diagnostics.push(diagnostic);
        }

        // 检查分号
        if (!trimmedLine.endsWith(';') &&
            !trimmedLine.endsWith('{') &&
            !trimmedLine.endsWith('}') &&
            !trimmedLine.startsWith('//') &&
            !trimmedLine.startsWith('/*') &&
            !trimmedLine.startsWith('*') &&
            !trimmedLine.startsWith('import') &&
            !trimmedLine.startsWith('package') &&
            !trimmedLine.startsWith('@') &&
            !trimmedLine.startsWith('#') &&  // 排除预处理器指令
            trimmedLine.length > 0) {
            // 检查是否是类或方法声明
            if (!trimmedLine.match(/^(?:public|private|protected)?\s*(?:static|final|abstract|native)?\s*(?:class|int|long|float|double|bool|string|char|void|\w+)\s+\w+\s*[{(]/)) {
                const range = new vscode.Range(lineNum, line.length - 1, lineNum, line.length);
                const diagnostic = new vscode.Diagnostic(
                    range,
                    "语句应该以分号结束",
                    vscode.DiagnosticSeverity.Error
                );
                diagnostic.code = 'missing-semicolon';
                diagnostics.push(diagnostic);
            }
        }

        return diagnostics;
    }

    /**
     * 运行检查器
     * @param document 文档
     * @param checkerPath 检查器路径
     * @returns 诊断数组
     */
    private async runChecker(
        document: vscode.TextDocument,
        checkerPath: string
    ): Promise<vscode.Diagnostic[]> {
        const diagnostics: vscode.Diagnostic[] = [];

        try {
            const { stdout, stderr } = await execAsync(
                `"${checkerPath}" "${document.fileName}"`,
                { timeout: 30000 }
            );

            const output = stdout || stderr;
            if (output) {
                const checkerDiagnostics = this.parseCheckerOutput(output, document);
                diagnostics.push(...checkerDiagnostics);
            }
        } catch (error: any) {
            // 处理 MultipleErrors 格式
            if (error.message && error.message.includes('MultipleErrors')) {
                const parsedErrors = this.parseMultipleErrors(error.message, document);
                diagnostics.push(...parsedErrors);
            } else if (error.stdout || error.stderr) {
                const output = error.stdout || error.stderr;
                const checkerDiagnostics = this.parseCheckerOutput(output, document);
                diagnostics.push(...checkerDiagnostics);
            }
        }

        return diagnostics;
    }

    /**
     * 解析 MultipleErrors 格式
     * @param errorMessage 错误消息
     * @param document 文档
     * @returns 诊断数组
     */
    private parseMultipleErrors(errorMessage: string, document: vscode.TextDocument): vscode.Diagnostic[] {
        const diagnostics: vscode.Diagnostic[] = [];

        // 匹配 MultipleErrors 中的各个错误
        // 格式: Semantic { file: None, line: 762, column: 28, message: "...", suggestion: "..." }
        const semanticPattern = /Semantic\s*\{\s*file:\s*(?:None|Some\("([^"]+)"\)),\s*line:\s*(\d+),\s*column:\s*(\d+),\s*message:\s*"([^"]+)"(?:,\s*suggestion:\s*"([^"]*)")?\s*\}/g;

        let match;
        while ((match = semanticPattern.exec(errorMessage)) !== null) {
            const [, filePath, lineStr, colStr, message, suggestion] = match;
            const lineNum = parseInt(lineStr, 10) - 1;
            const colNum = parseInt(colStr, 10) - 1;

            // 如果没有文件路径或文件路径匹配当前文档
            if (!filePath || document.fileName.includes(filePath) || filePath.includes(path.basename(document.fileName))) {
                const range = new vscode.Range(
                    lineNum,
                    Math.max(0, colNum),
                    lineNum,
                    Math.max(0, colNum) + 1
                );

                let fullMessage = message;
                if (suggestion) {
                    fullMessage += ` (${suggestion})`;
                }

                const diagnostic = new vscode.Diagnostic(
                    range,
                    fullMessage,
                    vscode.DiagnosticSeverity.Error
                );
                diagnostic.code = 'cavvy-semantic-error';
                diagnostics.push(diagnostic);
            }
        }

        // 也尝试匹配 Cavvy 编译器的错误格式
        // 格式: ╭─[.\hello.cay:5:28]
        const cavvyErrorPattern = /╭─\[(.+?):(\d+):(\d+)\][\s\S]*?×\s*\[E\d+\]\s*(.+?)(?=\n\s*╰────|$)/g;

        while ((match = cavvyErrorPattern.exec(errorMessage)) !== null) {
            const [, filePath, lineStr, colStr, message] = match;
            const lineNum = parseInt(lineStr, 10) - 1;
            const colNum = parseInt(colStr, 10) - 1;

            if (document.fileName.includes(filePath) || filePath.includes(path.basename(document.fileName))) {
                const range = new vscode.Range(
                    lineNum,
                    Math.max(0, colNum),
                    lineNum,
                    Math.max(0, colNum) + 1
                );

                const diagnostic = new vscode.Diagnostic(
                    range,
                    message.trim(),
                    vscode.DiagnosticSeverity.Error
                );
                diagnostic.code = 'cavvy-error';
                diagnostics.push(diagnostic);
            }
        }

        return diagnostics;
    }

    /**
     * 解析检查器输出
     * @param output 检查器输出
     * @param document 文档
     * @returns 诊断数组
     */
    private parseCheckerOutput(output: string, document: vscode.TextDocument): vscode.Diagnostic[] {
        const diagnostics: vscode.Diagnostic[] = [];
        const lines = output.split('\n');

        // 匹配常见的错误格式: file.cay:10:5: error: message
        const errorPattern = /(.+?):(\d+):(\d+):\s*(error|warning|note):\s*(.+)/i;

        // 匹配 Cavvy 编译器的错误格式: ╭─[file.cay:5:28]
        const cavvyErrorPattern = /╭─\[(.+?):(\d+):(\d+)\]/;

        for (let i = 0; i < lines.length; i++) {
            const line = lines[i];

            // 尝试匹配标准格式
            const match = errorPattern.exec(line);
            if (match) {
                const [, filePath, lineStr, colStr, severity, message] = match;
                const lineNum = parseInt(lineStr, 10) - 1;
                const colNum = parseInt(colStr, 10) - 1;

                if (document.fileName.includes(filePath) || filePath.includes(path.basename(document.fileName))) {
                    const range = new vscode.Range(
                        lineNum,
                        colNum,
                        lineNum,
                        colNum + 1
                    );

                    const diagnosticSeverity = this.parseSeverity(severity);
                    const diagnostic = new vscode.Diagnostic(range, message.trim(), diagnosticSeverity);
                    diagnostic.code = 'checker-error';
                    diagnostics.push(diagnostic);
                }
                continue;
            }

            // 尝试匹配 Cavvy 编译器格式
            const cavvyMatch = cavvyErrorPattern.exec(line);
            if (cavvyMatch) {
                const [, filePath, lineStr, colStr] = cavvyMatch;
                const lineNum = parseInt(lineStr, 10) - 1;
                const colNum = parseInt(colStr, 10) - 1;

                // 查找下一行的错误消息
                let message = '未知错误';
                for (let j = i + 1; j < lines.length && j < i + 10; j++) {
                    const msgLine = lines[j];
                    if (msgLine.includes('×') || msgLine.includes('error:')) {
                        message = msgLine.replace(/^[\s│·─╰]*[×]\s*/, '').replace(/error:\s*/i, '').trim();
                        break;
                    }
                }

                if (document.fileName.includes(filePath) || filePath.includes(path.basename(document.fileName))) {
                    const range = new vscode.Range(
                        lineNum,
                        Math.max(0, colNum),
                        lineNum,
                        Math.max(0, colNum) + 1
                    );

                    const diagnostic = new vscode.Diagnostic(range, message, vscode.DiagnosticSeverity.Error);
                    diagnostic.code = 'cavvy-error';
                    diagnostics.push(diagnostic);
                }
            }
        }

        return diagnostics;
    }

    /**
     * 解析严重级别
     * @param severity 严重级别字符串
     * @returns DiagnosticSeverity
     */
    private parseSeverity(severity: string): vscode.DiagnosticSeverity {
        switch (severity.toLowerCase()) {
            case 'error':
                return vscode.DiagnosticSeverity.Error;
            case 'warning':
                return vscode.DiagnosticSeverity.Warning;
            case 'note':
            case 'info':
                return vscode.DiagnosticSeverity.Information;
            default:
                return vscode.DiagnosticSeverity.Error;
        }
    }

    /**
     * 记录日志
     */
    private log(message: string): void {
        const timestamp = new Date().toISOString();
        this.outputChannel.appendLine(`[${timestamp}] ${message}`);
    }

    /**
     * 配置变更时的处理
     */
    onConfigurationChanged(): void {
        this.config = vscode.workspace.getConfiguration('cavvyAnalyzer');

        // 重新检查所有打开的文档
        vscode.workspace.textDocuments.forEach((doc) => {
            if (this.isCavvyFile(doc)) {
                this.scheduleCheck(doc);
            }
        });
    }

    /**
     * 释放资源
     */
    dispose(): void {
        this.log('释放诊断提供器资源');
        // 清除所有文档的定时器
        for (const timeout of this.documentTimeouts.values()) {
            clearTimeout(timeout);
        }
        this.documentTimeouts.clear();
        if (this.cleanupTimer) {
            clearInterval(this.cleanupTimer);
        }
        this.diagnosticCollection.dispose();
        this.disposables.forEach(d => d.dispose());
        this.outputChannel.dispose();
    }
}
