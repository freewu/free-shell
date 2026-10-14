# AGENTS.md

面向在本仓库工作的编码代理（以及人类协作者）的约定。

## 工作流（重要）

**每完成一项任务或一个可交付阶段，必须提交并推送到远程，不要留未提交的工作。**

```bash
git status --short          # 先确认没有多余产物（dist/、node_modules/、target/ 已被忽略）
git add -A
git commit -m "feat: ..."   # 类型：feat / fix / refactor / docs / test / chore / build
git push                    # origin/main
```

规则：

- **提交前先跑 `just verify`**（typecheck + cargo check --all-targets + fmt --check +
  clippy -D warnings + cargo test），必须全绿；改到 SSH 引擎、IPC 或界面时再加跑一次
  `just e2e-ui`。
- 一个逻辑变更一个 commit，不要把无关改动混在一起；提交信息用祈使句、英文或中文都可以，
  但同一仓库内保持一致（当前仓库以英文 `type: subject` 为主）。
- **绝不提交**密钥、密码、`%APPDATA%\com.freeshell.app` 下的会话文件、`screenshot.png`
  等临时产物。
- push 失败（网络 / 鉴权）时保留本地 commit，在回复里说明原因，**不要 force push**，
  也不要因此丢弃改动。
- 完成任务后的回复里附上 commit 短哈希与 push 结果。

## 项目速览

Tauri 2 + React 19 + Rust 的 SSH 客户端（类似 Xshell），带批量执行。

| 位置 | 作用 |
| --- | --- |
| `src/` | React 前端（zustand store、xterm.js 终端、批量面板） |
| `src/lib/ipc.ts` | 所有 `invoke` 命令的唯一入口，新增命令先加这里 |
| `src-tauri/src/ssh.rs` | `SshManager`：连接、shell 通道、`exec`、批量执行 |
| `src-tauri/src/store.rs` | 会话 / 设置 / known_hosts 的磁盘存储（原子写 + 坏文件隔离） |
| `src-tauri/src/commands.rs` | `#[tauri::command]` 层 |
| `src-tauri/src/models.rs` | 前后端共享结构体（**camelCase**） |
| `src/types.ts` | 与 Rust 侧对应的 TS 类型，改一边必须改另一边 |

后端事件：`ssh://data`（终端输出）、`ssh://state`（连接状态）、`batch://event`（批量进度）。

## 常用命令

```bash
just setup        # npm install + cargo fetch
just dev          # 桌面端开发（vite 热更新 + 原生窗口）
just build-debug  # 前端产物 + 内嵌资源的调试二进制（不依赖 dev server）
just verify       # 全部检查（提交前必跑）
just test         # 仅 Rust 测试（含端到端）
just e2e-ui       # 真实窗口 + CDP 的整机端到端（仅 Windows）
```

## 环境注意事项（Windows + WSL）

- WSL 里的 git 没有全局 `user.name` / `user.email`（Windows 侧有）。本仓库已设置
  repo-local 身份 `bluefrog <bluefrog.wu@gmail.com>`；若 `git commit` 报
  `Committer identity unknown`，先补上再提交，不要用 `--author` 之类临时绕过。
- 从 WSL 调 Windows 工具：`cmd.exe /c "cd /d E:\work\github\free-shell && <cmd>"`。
- **不要**把复杂 PowerShell 内联进 `-Command`（转义会把变量吃掉），写成 `.ps1` 后用
  `powershell -NoProfile -ExecutionPolicy Bypass -File <path>` 执行。
- `Set-Content -Encoding utf8` 会写入 BOM，`serde_json` 不接受；需要无 BOM 时用
  `[System.IO.File]::WriteAllText($path, $text, (New-Object System.Text.UTF8Encoding($false)))`。
- `ConvertTo-Json` 会把单元素数组拆开，喂给 Rust 时记得手动补 `[...]`。
- 前端 dev 端口固定 **1421**（`vite.config.ts` 的 `server.port` 与
  `src-tauri/tauri.conf.json` 的 `build.devUrl` 必须同步）。不要用 Tauri 默认的 1420：
  本机常有其它 Tauri 项目占用。
- `custom-protocol` feature 让调试二进制内嵌前端资源（`http://tauri.localhost/`），
  窗口里的 URL 变成 `tauri.localhost` 而不是 `localhost:1421` 是正常现象，不是 bug。

## 测试约定

- `src-tauri/tests/ssh_e2e.rs`：进程内 mini SSH 服务器 + `tauri::test::mock_app()`，
  覆盖连接、认证失败、TOFU、exec 的 stdout/stderr/退出码、超时、批量并发与失败隔离。
- `src-tauri/examples/fake_ssh_server.rs`：可独立启动的假 SSH 服务器，供 UI 联调与
  `scripts/ui_e2e.ps1` 使用；它理解的命令只有 `echo`、`whoami`、`hostname`、`uname`、
  `pwd`、`stderr`、`sleep`、`fail`、`env`、`exit`（**没有 shell 重定向**）。
- `just e2e-ui` 会临时改写 `%APPDATA%\com.freeshell.app` 下的会话文件，脚本已做备份与
  还原；新增用例时保持这个行为。
- 两个已知的产品限制，改代码时不要"顺手修"成假象：批量"遇错即停"依赖 `set -e`
  （只对 POSIX shell 有效）；密码目前明文存放在 `sessions.json`。
