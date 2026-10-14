# FreeShell

基于 **Tauri 2 + React 19 + Rust** 的跨平台 SSH 连接工具，类似 Xshell / FinalShell，
内置**批量执行**能力：一次把同一段脚本跑在一组主机上，结果实时回流到界面。

```
┌─────────────┬──────────────────────────────────────────────┐
│ 会话列表     │  终端 / 批量执行 标签页                       │
│ 分组 / 搜索  │                                              │
├─────────────┴──────────────────────────────────────────────┤
│ 状态栏：会话数 · 已连接 · 批量进度                            │
└────────────────────────────────────────────────────────────┘
```

## 功能

- **会话管理**：主机、端口、用户名、分组、颜色、标签、备注，落盘到 JSON，支持搜索与分组折叠。
- **认证方式**：密码、公钥（含 passphrase）、ssh-agent。
- **终端**：xterm.js（fit / search / web-links 插件），可调字号与字体，窗口尺寸变化自动 `resize`。
- **主机密钥校验**：TOFU（首次信任并记录指纹）或严格模式，指纹存放在 `known_hosts.json`。
- **批量执行**：多选主机 + 多行命令，可配置超时 / 并发数 / 统一密码 / 遇错即停，
  逐台流式回传 stdout、stderr、退出码与耗时，支持一键导出为 `.txt`。
- **连接保活 / 超时 / 横幅（banner）**：连接超时、keepalive 间隔、服务端 banner 均可在设置里调整。

## 技术栈

| 层 | 选型 |
| --- | --- |
| 外壳 | Tauri 2（`tauri.conf.json` 里 `frontendDist = ../dist`） |
| 前端 | React 19 + TypeScript + Vite 8 + zustand + xterm.js + lucide-react |
| 后端 | Rust 2021（`rust-version = 1.89`），`russh 0.64` + `ring`（纯 Rust，无需 cmake/nasm） |
| 任务 | `just`（`justfile`，Windows 下使用 PowerShell shell） |

## 目录结构

```
src/                     前端
  App.tsx                布局与弹窗编排
  components/            Sidebar / TabBar / TerminalPane / BatchPanel / StatusBar / *Modal
  store/useAppStore.ts   zustand store：bootstrap、连接、批量任务、事件订阅
  lib/ipc.ts             Tauri 命令薄封装
  lib/termBus.ts         终端数据总线（事件 → 具体 tab 的 xterm 实例）
src-tauri/
  src/ssh.rs             SshManager：连接池、shell 通道、exec、批量执行
  src/store.rs           会话 / 设置 / known_hosts 的磁盘存储
  src/commands.rs        #[tauri::command] 层
  src/models.rs          与前端共享的数据结构（camelCase）
  tests/ssh_e2e.rs       内嵌 mini SSH 服务器 + 7 个端到端测试
  examples/fake_ssh_server.rs  可独立启动的假 SSH 服务器（UI 联调/测试用）
scripts/ui_e2e.ps1       真机端到端：假服务器 + 真实窗口 + CDP 断言
```

## 开发

```bash
just setup      # npm install + cargo fetch
just dev        # 桌面应用（vite dev server + 热更新）
just dev-web    # 只跑浏览器端前端
```

> 桌面端调试构建默认加载 `build.devUrl`（`http://localhost:1421`）。
> 端口刻意避开 Tauri 默认的 1420，以免和本机其它 Tauri 项目抢端口；
> 若要修改，需要同时改 `vite.config.ts` 的 `server.port` 与 `src-tauri/tauri.conf.json`
> 的 `build.devUrl`。也可以直接用 `just build-debug` 构建一个自带前端资源的
> 二进制（启用 `custom-protocol` feature，完全不依赖 dev server）。

## 构建

```bash
just build-debug   # 前端产物 + 调试二进制（内嵌资源，可直接运行）
just build         # 生产包（tauri build，需要 NSIS/WiX 等打包工具）
just build-web     # 只构建 dist/
```

## 质量检查

```bash
just verify        # typecheck + cargo check --all-targets + clippy -D warnings + cargo test
just test          # 仅 Rust 测试（含端到端）
just lint          # cargo clippy --all-targets -- -D warnings
```

### Rust 端到端测试

`src-tauri/tests/ssh_e2e.rs` 会在随机端口拉起一个进程内的迷你 SSH 服务器，
再用真实的 `SshManager`（通过 `tauri::test::mock_app()` 注入 `MockRuntime`）连上去，
覆盖：连接/密码认证、错误密码被拒、主机指纹校验与 TOFU、`exec` 的
stdout/stderr/退出码、超时、批量执行的并发与失败隔离。

### 整机 UI 端到端测试

`scripts/ui_e2e.ps1` 会依次：启动 `examples/fake_ssh_server`，在应用数据目录写入一个
会话，用 `--remote-debugging-port` 启动真实窗口，然后通过 CDP 在页面内驱动 UI
（双击连接、键入命令、调用 IPC、点批量面板……），最后断言 25 项结果：

```powershell
just e2e-ui
```

## 数据文件

保存在 `%APPDATA%\com.freeshell.app`（Linux/macOS 为各自的 app data 目录）：

| 文件 | 内容 |
| --- | --- |
| `sessions.json` | 会话列表 |
| `settings.json` | 全局设置 |
| `known_hosts.json` | `host:port → 指纹` |

这几个文件是纯 JSON，可以手改；读取时容忍 BOM，解析失败会把文件重命名为
`*.corrupt` 并回退到默认值（不会导致应用起不来）。

## 说明与限制

- **"遇错即停"依赖 `set -e`**：批量执行在脚本前插入 `set -e`，因此该选项针对
  POSIX shell（bash/sh/zsh）有效；对 Windows 远端（cmd/PowerShell）会被忽略，
  命令会继续往下执行。
- **密码明文存储**：`sessions.json` 中的密码目前未加密，仅适合本机可信环境；
  不想落盘可以留空，连接时再输入（会弹出密码框）。
- 批量执行的并发上限为 64，超时范围 1s–24h。
