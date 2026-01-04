import * as vscode from 'vscode';

// 插件被激活时（比如打开了 .lm 文件）调用这个函数
// 这就像是 main.rs 中的 main()
export function activate(context: vscode.ExtensionContext) {
    console.log('Loom extension is now active!');

    // 注册一个简单的命令：Hello World
    // 你可以在 VSCode 中按下 Ctrl+Shift+P 输入 "Loom: Hello" 来测试
    let disposable = vscode.commands.registerCommand('loom.hello', () => {
        vscode.window.showInformationMessage('Hello from Loom Language!');
    });

    context.subscriptions.push(disposable);
}

// 插件停止时调用
export function deactivate() { }