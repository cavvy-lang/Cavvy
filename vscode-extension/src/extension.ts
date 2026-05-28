import * as vscode from 'vscode';
import * as path from 'path';
import { CavvyDefinitionProvider } from './providers/definitionProvider';
import { CavvyDiagnosticProvider } from './providers/diagnosticProvider';
import { CavvyDocumentSymbolProvider } from './providers/documentSymbolProvider';
import { CavvyReferenceProvider } from './providers/referenceProvider';
import { CavvyCompletionProvider } from './providers/completionProvider';
import { CavvyHoverProvider } from './providers/hoverProvider';
import { CavvyRunner } from './utils/runner';
import { CavvyLSPClient } from './utils/lspClient';

/**
 * Cavvy Analyzer 插件主入口
 * 提供语法高亮、跳转定义、语法错误诊断、一键运行、LSP支持等功能
 */

let diagnosticProvider: CavvyDiagnosticProvider | undefined;
let lspClient: CavvyLSPClient | undefined;
let runner: CavvyRunner | undefined;
let outputChannel: vscode.OutputChannel | undefined;

/**
 * 插件激活时调用
 * @param context 插件上下文
 */
export function activate(context: vscode.ExtensionContext): void {
    outputChannel = vscode.window.createOutputChannel('Cavvy Analyzer');
    log('Cavvy Analyzer 插件正在激活...');

    const config = vscode.workspace.getConfiguration('cavvyAnalyzer');

    try {
        // 初始化 LSP 客户端（使用 await 处理异步错误）
        if (config.get<boolean>('enableLSP', true)) {
            try {
                lspClient = new CavvyLSPClient();
                lspClient.activate(context).catch(err => {
                    log(`LSP 客户端激活失败: ${err}`);
                });
            } catch (err) {
                log(`LSP 客户端初始化失败: ${err}`);
            }
        }

        // 初始化运行器
        runner = new CavvyRunner();

        // 注册跳转到定义提供器
        const definitionProvider = vscode.languages.registerDefinitionProvider(
            'cavvy',
            new CavvyDefinitionProvider()
        );
        context.subscriptions.push(definitionProvider);

        // 注册文档符号提供器（用于大纲视图）
        const documentSymbolProvider = vscode.languages.registerDocumentSymbolProvider(
            'cavvy',
            new CavvyDocumentSymbolProvider()
        );
        context.subscriptions.push(documentSymbolProvider);

        // 注册查找引用提供器
        const referenceProvider = vscode.languages.registerReferenceProvider(
            'cavvy',
            new CavvyReferenceProvider()
        );
        context.subscriptions.push(referenceProvider);

        // 注册代码补全提供器
        const completionProvider = vscode.languages.registerCompletionItemProvider(
            'cavvy',
            new CavvyCompletionProvider(),
            '.',  // 触发字符：点号
            '(',  // 触发字符：左括号
            ':'   // 触发字符：冒号（用于方法引用）
        );
        context.subscriptions.push(completionProvider);

        // 注册 Hover 提供器
        const hoverProvider = vscode.languages.registerHoverProvider(
            'cavvy',
            new CavvyHoverProvider()
        );
        context.subscriptions.push(hoverProvider);

        // 初始化诊断提供器
        diagnosticProvider = new CavvyDiagnosticProvider();
        diagnosticProvider.activate(context);

        // 注册命令：运行代码（非调试模式）
        const runCodeNoDebugCommand = vscode.commands.registerCommand(
            'cavvyAnalyzer.runCodeNoDebug',
            async () => {
                const editor = vscode.window.activeTextEditor;
                if (!editor || !isCavvyFile(editor.document)) {
                    vscode.window.showWarningMessage('请先打开一个 Cavvy 文件 (.cay, .eol, .caybc, .ll)');
                    return;
                }

                // 先保存文件
                if (editor.document.isDirty) {
                    await editor.document.save();
                }

                await runner?.run(editor.document.fileName, { debug: false });
            }
        );
        context.subscriptions.push(runCodeNoDebugCommand);

        // 注册命令：运行代码（通用）
        const runCodeCommand = vscode.commands.registerCommand(
            'cavvyAnalyzer.runCode',
            async () => {
                const editor = vscode.window.activeTextEditor;
                if (!editor || !isCavvyFile(editor.document)) {
                    vscode.window.showWarningMessage('请先打开一个 Cavvy 文件');
                    return;
                }

                if (editor.document.isDirty) {
                    await editor.document.save();
                }

                await runner?.run(editor.document.fileName, { debug: false });
            }
        );
        context.subscriptions.push(runCodeCommand);

        // 注册命令：手动语法检查
        const checkSyntaxManualCommand = vscode.commands.registerCommand(
            'cavvyAnalyzer.checkSyntaxManual',
            async () => {
                const editor = vscode.window.activeTextEditor;
                if (!editor || !isCavvyFile(editor.document)) {
                    vscode.window.showWarningMessage('请先打开一个 Cavvy 文件');
                    return;
                }

                // 先保存文件
                if (editor.document.isDirty) {
                    await editor.document.save();
                }

                // 显示进度
                await vscode.window.withProgress({
                    location: vscode.ProgressLocation.Notification,
                    title: '正在检查 Cavvy 语法...',
                    cancellable: false
                }, async (progress) => {
                    progress.report({ increment: 0 });

                    // 使用 LSP 或本地诊断
                    if (lspClient?.isRunning()) {
                        // 触发 LSP 诊断
                        await lspClient?.triggerDiagnostics(editor.document);
                    } else {
                        // 使用本地诊断
                        await diagnosticProvider?.checkDocument(editor.document);
                    }

                    progress.report({ increment: 100 });
                });

                vscode.window.showInformationMessage('语法检查完成');
            }
        );
        context.subscriptions.push(checkSyntaxManualCommand);

        // 注册命令：检查语法（自动触发）- 修复：与 checkSyntaxManual 保持一致
        const checkSyntaxCommand = vscode.commands.registerCommand(
            'cavvyAnalyzer.checkSyntax',
            async () => {
                const editor = vscode.window.activeTextEditor;
                if (!editor || !isCavvyFile(editor.document)) {
                    vscode.window.showWarningMessage('请先打开一个 Cavvy 文件');
                    return;
                }

                // 先保存文件
                if (editor.document.isDirty) {
                    await editor.document.save();
                }

                // 显示进度
                await vscode.window.withProgress({
                    location: vscode.ProgressLocation.Notification,
                    title: '正在检查 Cavvy 语法...',
                    cancellable: false
                }, async (progress) => {
                    progress.report({ increment: 0 });

                    // 使用 LSP 或本地诊断
                    if (lspClient?.isRunning()) {
                        await lspClient?.triggerDiagnostics(editor.document);
                    } else {
                        await diagnosticProvider?.checkDocument(editor.document);
                    }

                    progress.report({ increment: 100 });
                });

                vscode.window.showInformationMessage('语法检查完成');
            }
        );
        context.subscriptions.push(checkSyntaxCommand);

        // 注册命令：跳转到定义（用于右键菜单）
        const gotoDefinitionCommand = vscode.commands.registerCommand(
            'cavvyAnalyzer.gotoDefinition',
            async () => {
                const editor = vscode.window.activeTextEditor;
                if (!editor || !isCavvyFile(editor.document)) {
                    return;
                }

                const position = editor.selection.active;
                const locations = await new CavvyDefinitionProvider().provideDefinition(
                    editor.document,
                    position,
                    new vscode.CancellationTokenSource().token
                );

                if (locations && locations.length > 0) {
                    const location = locations[0] as vscode.Location;
                    await vscode.window.showTextDocument(location.uri, {
                        selection: location.range
                    });
                } else {
                    vscode.window.showInformationMessage('未找到定义');
                }
            }
        );
        context.subscriptions.push(gotoDefinitionCommand);

        // 注册命令：重启 LSP 服务器
        const restartLSPCommand = vscode.commands.registerCommand(
            'cavvyAnalyzer.restartLSP',
            async () => {
                if (lspClient) {
                    await lspClient.restart(context);
                    vscode.window.showInformationMessage('Cavvy LSP 服务器已重启');
                } else {
                    // 创建新的 LSP 客户端
                    lspClient = new CavvyLSPClient();
                    await lspClient.activate(context);
                    vscode.window.showInformationMessage('Cavvy LSP 服务器已启动');
                }
            }
        );
        context.subscriptions.push(restartLSPCommand);

        // 注册命令：停止 LSP 服务器
        const stopLSPCommand = vscode.commands.registerCommand(
            'cavvyAnalyzer.stopLSP',
            async () => {
                if (lspClient) {
                    await lspClient.stop();
                    vscode.window.showInformationMessage('Cavvy LSP 服务器已停止');
                }
            }
        );
        context.subscriptions.push(stopLSPCommand);

        // 监听文档打开事件，为空文件生成模板
        const onDidOpenDisposable = vscode.workspace.onDidOpenTextDocument(
            (document) => {
                if (isCavvyFile(document) && document.getText().trim().length === 0) {
                    generateTemplate(document);
                }
            }
        );
        context.subscriptions.push(onDidOpenDisposable);

        // 检查当前已打开的空文档
        vscode.workspace.textDocuments.forEach((doc) => {
            if (isCavvyFile(doc) && doc.getText().trim().length === 0) {
                generateTemplate(doc);
            }
        });

        // 监听配置变更
        const configChangeDisposable = vscode.workspace.onDidChangeConfiguration(
            (event) => {
                if (event.affectsConfiguration('cavvyAnalyzer')) {
                    diagnosticProvider?.onConfigurationChanged();
                    runner?.onConfigurationChanged();

                    // 检查 LSP 配置变更
                    if (event.affectsConfiguration('cavvyAnalyzer.enableLSP') ||
                        event.affectsConfiguration('cavvyAnalyzer.lspServerPath')) {
                        const newConfig = vscode.workspace.getConfiguration('cavvyAnalyzer');
                        const enableLSP = newConfig.get<boolean>('enableLSP', true);

                        if (enableLSP && !lspClient?.isRunning()) {
                            lspClient = new CavvyLSPClient();
                            lspClient.activate(context);
                        } else if (!enableLSP && lspClient?.isRunning()) {
                            lspClient.stop();
                        }
                    }
                }
            }
        );
        context.subscriptions.push(configChangeDisposable);

        // 显示激活成功消息
        const lspStatus = lspClient?.isRunning() ? '已连接' : '未启用';
        vscode.window.showInformationMessage(`Cavvy Analyzer 已激活 (LSP: ${lspStatus})`);
        log(`Cavvy Analyzer 插件已激活 (LSP: ${lspStatus})`);

    } catch (error) {
        log(`插件激活失败: ${error}`);
        vscode.window.showErrorMessage(`Cavvy Analyzer 激活失败: ${error}`);
        throw error;
    }
}

/**
 * 检查文档是否是 Cavvy 文件
 * @param document 文档
 * @returns 是否是 Cavvy 文件
 */
function isCavvyFile(document: vscode.TextDocument): boolean {
    return document.languageId === 'cavvy' ||
           document.fileName.endsWith('.cay') ||
           document.fileName.endsWith('.eol') ||
           document.fileName.endsWith('.caybc') ||
           document.fileName.endsWith('.ll');
}

/**
 * 为空的 .cay 文件生成模板代码
 * @param document 文档
 */
async function generateTemplate(document: vscode.TextDocument): Promise<void> {
    // 只处理 .cay 文件
    if (!document.fileName.endsWith('.cay')) {
        return;
    }

    // 获取文件名（不含扩展名）
    const fileName = path.basename(document.fileName, '.cay');

    // 转换为 PascalCase（大驼峰形式）
    const className = toPascalCase(fileName);

    // 生成模板代码
    const template = `@main
public class ${className} {
    public static void main() {
        // 在这里写入你的代码

    }
}`;

    // 获取编辑器
    const editor = await vscode.window.showTextDocument(document);

    // 插入模板
    await editor.edit((editBuilder) => {
        editBuilder.insert(new vscode.Position(0, 0), template);
    });

    // 将光标定位到注释后的位置（在 main 方法体内）
    const position = new vscode.Position(4, 8);  // 注释行的下一行，缩进位置
    editor.selection = new vscode.Selection(position, position);
}

/**
 * 将字符串转换为 PascalCase（大驼峰形式）
 * @param str 输入字符串
 * @returns PascalCase 形式的字符串
 */
function toPascalCase(str: string): string {
    // 如果字符串已经是全大写或全小写，首字母大写即可
    if (/^[a-z]+$/.test(str)) {
        return str.charAt(0).toUpperCase() + str.slice(1);
    }
    if (/^[A-Z]+$/.test(str)) {
        return str.charAt(0).toUpperCase() + str.slice(1).toLowerCase();
    }

    // 处理下划线、连字符或空格分隔的字符串
    return str
        .replace(/[-_]/g, ' ')
        .replace(/\s+(.)/g, (_, char) => char.toUpperCase())
        .replace(/^[a-z]/, (char) => char.toUpperCase())
        .replace(/\s+/g, '');
}

/**
 * 记录日志
 */
function log(message: string): void {
    const timestamp = new Date().toISOString();
    outputChannel?.appendLine(`[${timestamp}] ${message}`);
}

/**
 * 插件停用时调用
 */
export function deactivate(): void {
    log('Cavvy Analyzer 插件正在停用...');
    diagnosticProvider?.dispose();
    lspClient?.dispose();
    runner?.dispose();
    diagnosticProvider = undefined;
    lspClient = undefined;
    runner = undefined;
    outputChannel?.dispose();
    outputChannel = undefined;
}
