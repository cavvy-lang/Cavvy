import * as vscode from 'vscode';
import { LanguageClient, LanguageClientOptions, ServerOptions, TransportKind } from 'vscode-languageclient/node';
import * as path from 'path';
import * as fs from 'fs';
import { spawn, ChildProcess } from 'child_process';

/**
 * Cavvy LSP 客户端
 * 用于与 cay-lsp 语言服务器通信
 */
export class CavvyLSPClient {
    private client: LanguageClient | undefined;
    private config: vscode.WorkspaceConfiguration;
    private isActive: boolean = false;
    private outputChannel: vscode.OutputChannel;
    private context: vscode.ExtensionContext | undefined;
    private serverProcess: ChildProcess | undefined;

    constructor() {
        this.config = vscode.workspace.getConfiguration('cavvyAnalyzer');
        this.outputChannel = vscode.window.createOutputChannel('Cavvy LSP');
    }

    /**
     * 激活 LSP 客户端
     * @param context 插件上下文
     */
    async activate(context: vscode.ExtensionContext): Promise<void> {
        this.context = context;
        const enableLSP = this.config.get<boolean>('enableLSP', true);

        if (!enableLSP) {
            this.log('LSP 被禁用');
            return;
        }

        // 查找 LSP 服务器路径
        const lspServerPath = await this.findLspServerPath();
        
        if (!lspServerPath) {
            this.log('未找到 cay-lsp 可执行文件');
            vscode.window.showWarningMessage(
                'Cavvy LSP 服务器 (cay-lsp) 未找到。请确保已安装 Cavvy 编译器工具链。',
                '查看安装指南'
            ).then(selection => {
                if (selection === '查看安装指南') {
                    vscode.env.openExternal(vscode.Uri.parse('https://github.com/ethernos-studio/cavvy#installation'));
                }
            });
            return;
        }

        // 检查 cay-lsp 是否可用
        const isAvailable = await this.checkLspServer(lspServerPath);
        if (!isAvailable) {
            this.log(`cay-lsp 检查失败: ${lspServerPath}`);
            vscode.window.showWarningMessage(
                `Cavvy LSP 服务器 (${lspServerPath}) 无法启动。某些功能可能受限。`,
                '禁用 LSP', '查看设置'
            ).then(selection => {
                if (selection === '禁用 LSP') {
                    this.config.update('enableLSP', false, true);
                } else if (selection === '查看设置') {
                    vscode.commands.executeCommand('workbench.action.openSettings', 'cavvyAnalyzer.lspServerPath');
                }
            });
            return;
        }

        try {
            // 配置服务器选项 - 使用stdio传输
            const serverOptions: ServerOptions = {
                command: lspServerPath,
                args: [],
                transport: TransportKind.stdio
            };

            // 配置客户端选项
            const clientOptions: LanguageClientOptions = {
                documentSelector: [
                    { scheme: 'file', language: 'cavvy' },
                    { scheme: 'file', pattern: '**/*.cay' },
                    { scheme: 'file', pattern: '**/*.eol' },
                    { scheme: 'file', pattern: '**/*.caybc' },
                    { scheme: 'file', pattern: '**/*.ll' }
                ],
                synchronize: {
                    fileEvents: vscode.workspace.createFileSystemWatcher('**/*.cay')
                },
                outputChannel: this.outputChannel,
                revealOutputChannelOn: 4 // never
            };

            // 创建语言客户端
            this.client = new LanguageClient(
                'cavvyLSP',
                'Cavvy Language Server',
                serverOptions,
                clientOptions
            );

            // 监听客户端状态变化
            this.client.onDidChangeState((event) => {
                this.log(`LSP 状态变化: ${event.oldState} -> ${event.newState}`);
                if (event.newState === 2) { // Running
                    this.isActive = true;
                    this.log('LSP 客户端已连接并运行');
                } else if (event.newState === 3) { // Stopped
                    this.isActive = false;
                    this.log('LSP 客户端已停止');
                }
            });

            // 启动客户端
            await this.client.start();
            this.isActive = true;

            this.log('Cavvy LSP 客户端已启动');

            // 注册到上下文
            context.subscriptions.push({
                dispose: () => {
                    this.dispose();
                }
            });

            // 显示成功消息
            vscode.window.showInformationMessage('Cavvy LSP 服务器已连接');

        } catch (error) {
            this.log(`启动 LSP 客户端失败: ${error}`);
            console.error('启动 LSP 客户端失败:', error);
            vscode.window.showErrorMessage(`启动 Cavvy LSP 失败: ${error}`);
            this.isActive = false;
        }
    }

    /**
     * 查找 LSP 服务器路径
     * 按优先级顺序查找：配置路径 -> 项目target/release -> PATH
     */
    private async findLspServerPath(): Promise<string | undefined> {
        // 1. 检查配置路径
        const configPath = this.config.get<string>('lspServerPath', 'cay-lsp');
        if (configPath && configPath !== 'cay-lsp') {
            if (fs.existsSync(configPath)) {
                this.log(`使用配置的 LSP 路径: ${configPath}`);
                return configPath;
            }
        }

        // 2. 检查项目 target/release 目录
        if (this.context) {
            const workspacePath = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
            if (workspacePath) {
                const projectLspPath = path.join(workspacePath, 'target', 'release', 'cay-lsp.exe');
                if (fs.existsSync(projectLspPath)) {
                    this.log(`使用项目 LSP 路径: ${projectLspPath}`);
                    return projectLspPath;
                }
                const projectLspPathNoExt = path.join(workspacePath, 'target', 'release', 'cay-lsp');
                if (fs.existsSync(projectLspPathNoExt)) {
                    this.log(`使用项目 LSP 路径: ${projectLspPathNoExt}`);
                    return projectLspPathNoExt;
                }
            }
        }

        // 3. 检查环境变量 CAVVY_HOME
        const cavvyHome = process.env.CAVVY_HOME;
        if (cavvyHome) {
            const cavvyLspPath = path.join(cavvyHome, 'cay-lsp.exe');
            if (fs.existsSync(cavvyLspPath)) {
                this.log(`使用 CAVVY_HOME LSP 路径: ${cavvyLspPath}`);
                return cavvyLspPath;
            }
            const cavvyLspPathNoExt = path.join(cavvyHome, 'cay-lsp');
            if (fs.existsSync(cavvyLspPathNoExt)) {
                this.log(`使用 CAVVY_HOME LSP 路径: ${cavvyLspPathNoExt}`);
                return cavvyLspPathNoExt;
            }
        }

        // 4. 返回默认命令，让系统从 PATH 中查找
        this.log('使用默认 LSP 命令: cay-lsp');
        return 'cay-lsp';
    }

    /**
     * 检查 LSP 服务器是否可用
     * @param serverPath 服务器路径
     */
    private async checkLspServer(serverPath: string): Promise<boolean> {
        return new Promise((resolve) => {
            try {
                // 尝试运行 cay-lsp --version
                const proc = spawn(serverPath, ['--version'], {
                    timeout: 5000,
                    shell: true
                });

                let stdout = '';
                let stderr = '';

                proc.stdout?.on('data', (data) => {
                    stdout += data.toString();
                });

                proc.stderr?.on('data', (data) => {
                    stderr += data.toString();
                });

                proc.on('close', (code) => {
                    if (code === 0) {
                        this.log(`LSP 服务器版本: ${stdout.trim()}`);
                        resolve(true);
                    } else {
                        this.log(`LSP 服务器检查失败，退出码: ${code}`);
                        resolve(false);
                    }
                });

                proc.on('error', (err) => {
                    this.log(`LSP 服务器检查错误: ${err.message}`);
                    resolve(false);
                });

                // 设置超时
                setTimeout(() => {
                    proc.kill();
                    this.log('LSP 服务器检查超时');
                    resolve(false);
                }, 5000);

            } catch (error) {
                this.log(`检查 LSP 服务器失败: ${error}`);
                resolve(false);
            }
        });
    }

    /**
     * 触发文档诊断
     * @param document 文档
     */
    async triggerDiagnostics(document: vscode.TextDocument): Promise<void> {
        if (!this.client || !this.isActive) {
            this.log('LSP 客户端未运行，无法触发诊断');
            return;
        }

        try {
            this.log(`触发文档诊断: ${document.uri.toString()}`);
            
            // 发送文档打开通知
            await this.client.sendNotification('textDocument/didOpen', {
                textDocument: {
                    uri: document.uri.toString(),
                    languageId: document.languageId,
                    version: document.version,
                    text: document.getText()
                }
            });

            // 发送文档内容变更通知以触发诊断
            await this.client.sendNotification('textDocument/didChange', {
                textDocument: {
                    uri: document.uri.toString(),
                    version: document.version
                },
                contentChanges: [
                    {
                        text: document.getText()
                    }
                ]
            });

            this.log('诊断通知已发送');
        } catch (error) {
            this.log(`触发诊断失败: ${error}`);
            console.error('触发诊断失败:', error);
        }
    }

    /**
     * 重启 LSP 服务器
     */
    async restart(context?: vscode.ExtensionContext): Promise<void> {
        this.log('重启 LSP 服务器...');
        
        if (this.client) {
            try {
                await this.client.stop();
            } catch (error) {
                this.log(`停止 LSP 客户端时出错: ${error}`);
            }
            this.isActive = false;
        }

        // 重新创建配置
        this.config = vscode.workspace.getConfiguration('cavvyAnalyzer');

        // 重新激活
        if (context) {
            this.context = context;
        }
        
        if (this.context) {
            await this.activate(this.context);
        }
    }

    /**
     * 停止 LSP 服务器
     */
    async stop(): Promise<void> {
        if (this.client) {
            try {
                await this.client.stop();
                this.log('Cavvy LSP 客户端已停止');
            } catch (error) {
                this.log(`停止 LSP 客户端时出错: ${error}`);
            }
            this.isActive = false;
        }
    }

    /**
     * 检查 LSP 是否正在运行
     */
    isRunning(): boolean {
        return this.isActive && this.client !== undefined;
    }

    /**
     * 获取语言客户端
     */
    getClient(): LanguageClient | undefined {
        return this.client;
    }

    /**
     * 配置变更时的处理
     */
    onConfigurationChanged(): void {
        this.config = vscode.workspace.getConfiguration('cavvyAnalyzer');
    }

    /**
     * 记录日志
     */
    private log(message: string): void {
        const timestamp = new Date().toISOString();
        this.outputChannel.appendLine(`[${timestamp}] ${message}`);
    }

    /**
     * 释放资源
     */
    dispose(): void {
        this.log('释放 LSP 客户端资源');
        if (this.client) {
            this.client.stop();
            this.client = undefined;
        }
        this.isActive = false;
        this.outputChannel.dispose();
    }
}
