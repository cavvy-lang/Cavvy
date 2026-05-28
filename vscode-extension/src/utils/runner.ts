import * as vscode from 'vscode';
import { exec, spawn } from 'child_process';
import { promisify } from 'util';
import * as path from 'path';
import * as fs from 'fs';
import * as os from 'os';

const execAsync = promisify(exec);

/**
 * 运行选项
 */
export interface RunOptions {
    debug?: boolean;
    verbose?: boolean;
    outputFile?: string;
    noRun?: boolean;
}

/**
 * Cavvy 代码运行器
 * 支持使用 cay-run 运行 Cavvy 代码
 */
export class CavvyRunner {
    private config: vscode.WorkspaceConfiguration;
    private outputChannel: vscode.OutputChannel;
    private terminal: vscode.Terminal | undefined;
    private isPowerShell: boolean;

    constructor() {
        this.config = vscode.workspace.getConfiguration('cavvyAnalyzer');
        this.outputChannel = vscode.window.createOutputChannel('Cavvy Runner');
        this.isPowerShell = this.detectPowerShell();
    }

    /**
     * 检测当前系统是否使用 PowerShell
     */
    private detectPowerShell(): boolean {
        const shell = vscode.env.shell;
        if (shell) {
            const shellLower = shell.toLowerCase();
            return shellLower.includes('powershell') || shellLower.includes('pwsh');
        }
        // 默认 Windows 使用 PowerShell
        return os.platform() === 'win32';
    }

    /**
     * 转义参数以适配 PowerShell
     * @param arg 参数
     * @returns 转义后的参数
     */
    private escapeArgForPowerShell(arg: string): string {
        // PowerShell 参数转义：
        // 1. 如果参数包含空格或特殊字符，需要用引号包裹
        // 2. 内部的双引号需要转义为 `"`
        if (arg.includes(' ') || arg.includes('"') || arg.includes('(') || arg.includes(')')) {
            // eslint-disable-next-line no-useless-escape
            return `"${arg.replace(/"/g, '\`"')}"`;
        }
        return arg;
    }

    /**
     * 转义参数以适配 CMD
     * @param arg 参数
     * @returns 转义后的参数
     */
    private escapeArgForCmd(arg: string): string {
        // CMD 参数转义
        if (arg.includes(' ') || arg.includes('"')) {
            return `"${arg.replace(/"/g, '\\"')}"`;
        }
        return arg;
    }

    /**
     * 构建命令数组
     * @param runnerPath 运行器路径
     * @param args 参数列表
     * @returns 命令字符串
     */
    private buildCommand(runnerPath: string, args: string[]): string {
        const escapedPath = this.isPowerShell
            ? this.escapeArgForPowerShell(runnerPath)
            : this.escapeArgForCmd(runnerPath);

        const escapedArgs = args.map(arg =>
            this.isPowerShell
                ? this.escapeArgForPowerShell(arg)
                : this.escapeArgForCmd(arg)
        );

        return `${escapedPath} ${escapedArgs.join(' ')}`;
    }

    /**
     * 使用 spawn 执行命令（更好的跨平台兼容性）
     * @param command 命令
     * @param args 参数数组
     * @param cwd 工作目录
     * @returns Promise<{stdout: string; stderr: string}>
     */
    private spawnAsync(command: string, args: string[], cwd: string): Promise<{stdout: string; stderr: string}> {
        return new Promise((resolve, reject) => {
            const stdout: Buffer[] = [];
            const stderr: Buffer[] = [];

            // Windows 需要使用 shell 模式来正确解析命令
            const isWindows = os.platform() === 'win32';
            const spawnOptions = {
                cwd,
                shell: isWindows,
                windowsVerbatimArguments: isWindows
            };

            const child = spawn(command, args, spawnOptions);

            child.stdout?.on('data', (data) => {
                stdout.push(Buffer.from(data));
            });

            child.stderr?.on('data', (data) => {
                stderr.push(Buffer.from(data));
            });

            child.on('close', (code) => {
                const result = {
                    stdout: Buffer.concat(stdout).toString('utf-8'),
                    stderr: Buffer.concat(stderr).toString('utf-8')
                };

                if (code === 0) {
                    resolve(result);
                } else {
                    const error = new Error(`命令退出码: ${code}`) as Error & { stdout: string; stderr: string };
                    error.stdout = result.stdout;
                    error.stderr = result.stderr;
                    reject(error);
                }
            });

            child.on('error', (error) => {
                reject(error);
            });

            // 超时处理
            const timeout = setTimeout(() => {
                child.kill();
                reject(new Error('命令执行超时'));
            }, 60000);

            child.on('close', () => {
                clearTimeout(timeout);
            });
        });
    }

    /**
     * 运行 Cavvy 文件
     * @param filePath 文件路径
     * @param options 运行选项
     */
    async run(filePath: string, options: RunOptions = {}): Promise<void> {
        const runnerPath = this.config.get<string>('runnerPath', 'cay-run');
        const runInTerminal = this.config.get<boolean>('runInTerminal', true);
        const preserveFocus = this.config.get<boolean>('preserveFocus', false);

        // 检查文件是否存在
        if (!fs.existsSync(filePath)) {
            vscode.window.showErrorMessage(`文件不存在: ${filePath}`);
            return;
        }

        // 检查文件类型
        const ext = path.extname(filePath).toLowerCase();
        const supportedExts = ['.cay', '.eol', '.caybc', '.ll'];
        if (!supportedExts.includes(ext)) {
            vscode.window.showErrorMessage(`不支持的文件类型: ${ext}。支持的类型: ${supportedExts.join(', ')}`);
            return;
        }

        try {
            // 构建命令参数（不包含文件路径，单独处理）
            const args: string[] = [];
            if (options.verbose) {
                args.push('--verbose');
            }
            if (options.noRun) {
                args.push('--no-run');
            }
            if (options.outputFile) {
                args.push('-o', options.outputFile);
            }
            // 文件路径作为独立参数，不进行预转义
            args.push(filePath);

            const command = this.buildCommand(runnerPath, args);

            if (runInTerminal) {
                // 在终端中运行（支持交互式输入）
                await this.runInTerminal(command, filePath, preserveFocus);
            } else {
                // 在输出通道中运行
                await this.runInOutputChannelSpawn(runnerPath, args, filePath);
            }
        } catch (error) {
            vscode.window.showErrorMessage(`运行失败: ${error}`);
        }
    }

    /**
     * 在终端中运行
     * @param command 命令
     * @param filePath 文件路径
     * @param preserveFocus 是否保持焦点
     */
    private async runInTerminal(command: string, filePath: string, preserveFocus: boolean): Promise<void> {
        const fileName = path.basename(filePath);

        // 如果终端已存在且未关闭，则复用
        if (this.terminal) {
            try {
                this.terminal.show(preserveFocus);
                this.terminal.sendText(command);
                return;
            } catch {
                // 终端可能已关闭，创建新终端
                this.terminal = undefined;
            }
        }

        // 创建新终端
        this.terminal = vscode.window.createTerminal({
            name: `Cavvy: ${fileName}`,
            cwd: path.dirname(filePath)
        });

        this.terminal.show(preserveFocus);
        this.terminal.sendText(command);

        // 监听终端关闭事件
        const dispose = vscode.window.onDidCloseTerminal((t) => {
            if (t === this.terminal) {
                this.terminal = undefined;
                dispose.dispose();
            }
        });
    }

    /**
     * 在输出通道中运行
     * @param command 命令
     * @param filePath 文件路径
     */
    private async runInOutputChannel(command: string, filePath: string): Promise<void> {
        const fileName = path.basename(filePath);

        this.outputChannel.clear();
        this.outputChannel.show(true);
        this.outputChannel.appendLine(`运行: ${fileName}`);
        this.outputChannel.appendLine(`命令: ${command}`);
        this.outputChannel.appendLine('─'.repeat(50));

        try {
            const { stdout, stderr } = await execAsync(command, {
                timeout: 60000,
                cwd: path.dirname(filePath)
            });

            if (stdout) {
                this.outputChannel.appendLine(stdout);
            }
            if (stderr) {
                this.outputChannel.appendLine('标准错误输出:');
                this.outputChannel.appendLine(stderr);
            }

            this.outputChannel.appendLine('─'.repeat(50));
            this.outputChannel.appendLine('程序执行完成');
        } catch (error) {
            const err = error as Error & { stdout?: string; stderr?: string };
            this.outputChannel.appendLine('执行出错:');
            this.outputChannel.appendLine(err.message || String(error));

            if (err.stdout) {
                this.outputChannel.appendLine('标准输出:');
                this.outputChannel.appendLine(err.stdout);
            }
            if (err.stderr) {
                this.outputChannel.appendLine('标准错误:');
                this.outputChannel.appendLine(err.stderr);
            }

            vscode.window.showErrorMessage(`运行失败: ${err.message || String(error)}`);
        }
    }

    /**
     * 使用 spawn 在输出通道中运行（更好的跨平台兼容性）
     * @param command 命令
     * @param args 参数数组
     * @param filePath 文件路径
     */
    private async runInOutputChannelSpawn(command: string, args: string[], filePath: string): Promise<void> {
        this.outputChannel.clear();
        this.outputChannel.show(true);
        this.outputChannel.appendLine(`运行: ${path.basename(filePath)}`);
        this.outputChannel.appendLine(`命令: ${command} ${args.join(' ')}`);
        this.outputChannel.appendLine('─'.repeat(50));

        try {
            const { stdout, stderr } = await this.spawnAsync(command, args, path.dirname(filePath));

            if (stdout) {
                this.outputChannel.appendLine(stdout);
            }
            if (stderr) {
                this.outputChannel.appendLine('标准错误输出:');
                this.outputChannel.appendLine(stderr);
            }

            this.outputChannel.appendLine('─'.repeat(50));
            this.outputChannel.appendLine('程序执行完成');
        } catch (error) {
            const err = error as Error & { stdout?: string; stderr?: string };
            this.outputChannel.appendLine('执行出错:');
            this.outputChannel.appendLine(err.message || String(error));

            if (err.stdout) {
                this.outputChannel.appendLine('标准输出:');
                this.outputChannel.appendLine(err.stdout);
            }
            if (err.stderr) {
                this.outputChannel.appendLine('标准错误:');
                this.outputChannel.appendLine(err.stderr);
            }

            vscode.window.showErrorMessage(`运行失败: ${err.message || String(error)}`);
        }
    }

    /**
     * 编译代码（不运行）
     * @param filePath 文件路径
     * @param outputFile 输出文件路径
     */
    async compile(filePath: string, outputFile?: string): Promise<boolean> {
        const runnerPath = this.config.get<string>('runnerPath', 'cay-run');

        try {
            const args: string[] = ['--no-run'];
            if (outputFile) {
                args.push('-o', outputFile);
            }
            args.push(filePath);

            const { stderr } = await this.spawnAsync(runnerPath, args, path.dirname(filePath));

            if (stderr) {
                vscode.window.showWarningMessage(`编译警告: ${stderr}`);
            }

            vscode.window.showInformationMessage('编译成功');
            return true;
        } catch (error) {
            const err = error as Error & { stdout?: string; stderr?: string };
            vscode.window.showErrorMessage(`编译失败: ${err.message || String(error)}`);
            return false;
        }
    }

    /**
     * 检查 cay-run 是否可用
     */
    async checkRunner(): Promise<boolean> {
        const runnerPath = this.config.get<string>('runnerPath', 'cay-run');

        try {
            await this.spawnAsync(runnerPath, ['--version'], process.cwd());
            return true;
        } catch {
            return false;
        }
    }

    /**
     * 配置变更时的处理
     */
    onConfigurationChanged(): void {
        this.config = vscode.workspace.getConfiguration('cavvyAnalyzer');
    }

    /**
     * 释放资源
     */
    dispose(): void {
        if (this.terminal) {
            this.terminal.dispose();
            this.terminal = undefined;
        }
        this.outputChannel.dispose();
    }
}
