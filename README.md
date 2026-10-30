# Windows MCP Runtime Repair

为 Windows x64 的 Codex / ChatGPT 桌面应用提供版本感知的 MCP 标准流修复。

保留原应用入口。安装后，内部后端入口自动识别启动它的应用包，只使用相同后端版本的修复程序。系统完成应用部署时会提前准备对应运行文件；处理结束即退出，不安装常驻监视器或登录启动项。

## 使用

从本项目 Releases 下载并解压工具，保持两个程序位于同一目录。在普通用户终端中执行：

```powershell
.\mcp-runtime-repair.exe install
.\mcp-runtime-repair.exe status
```

完全退出桌面应用，再从原来的开始菜单入口打开。无需新增“修复入口”。

可以指定下载中间文件的目录：

```powershell
.\mcp-runtime-repair.exe --temporary-directory E:\RuntimeDownloads install
```

手动提前准备当前版本，或移除本项目设置的入口：

```powershell
.\mcp-runtime-repair.exe prepare
.\mcp-runtime-repair.exe uninstall
```

卸载只恢复本项目接管的用户级启动设置，保留正在使用的缓存文件。它不会覆盖后来由其他程序设置的后端入口。

## 更新机制

```mermaid
flowchart LR
  A[官方应用完成更新] --> B[一次性准备任务]
  B --> C[识别实际后端版本]
  C --> D[匹配公开运行目录]
  D --> E[校验并缓存同版程序]
  E --> F[原应用入口启动]
  F --> G[内部版本选择器]
  G --> H[匹配的后端]
```

公开构建流程按官方 Windows 分发包识别后端版本，从同名公开标签构建补丁程序，并更新运行目录。版本不匹配时不会拿旧后端替代新包。程序、参数、工作目录、标准流和退出码沿原链路传递。

部署任务同时恢复丢失或被重置为当前官方后端的入口设置。已改为其他后端或插件来源的设置会保留，由安装状态决定是否继续接管。

应用包中的配套程序与插件文件只从本机已安装的同版包读取，不随本项目重新分发。镜像文件按普通字节写入，避免继承安装包的加密文件属性。

发行目录包含后端 `0.165.0-alpha.1.2` 和 `0.163.0-alpha.10.2`。后续版本由精确标签构建流程维护；发现新版本后，需要等待对应载荷发布。源码接口变化时，构建流程会停止发布，保留版本匹配要求。

## 修复内容

- Windows MCP 父端标准流改用 overlapped named pipes / IOCP，保留子进程的同步标准句柄和进程生命周期。
- 保留 MCP、现有并发设置及协议帧限制，不改聊天历史和 SQLite 数据库。
- WebSocket Close 1009 保留为明确错误类型，由现有单会话回退路径改用 HTTP。

对应上游报告：[Windows stdio 阻塞池问题](https://github.com/openai/codex/issues/42740)、[跨对话路由与队列停滞](https://github.com/openai/codex/issues/50923)。

## 构建

工具使用 Rust；Windows 运行文件由 `tools/upstream/Build-PatchedRuntime.ps1` 从精确匹配的公开标签构建。GitHub Actions 支持按官方分发包自动发现，或显式指定后端版本。

```powershell
cargo build --release
```

项目采用 Apache-2.0。补丁运行程序保留上游 `openai/codex` 的许可证、版权和 NOTICE；本项目不分发商店专有程序或本机配置。
