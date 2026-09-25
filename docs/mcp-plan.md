# MCP Server 规划（让外部 AI 调用 LaterMD 的文档检索能力）

日期：2026-09-25
状态：规划中（**本轮不实现代码**，设置页显示「规划中」禁用态）
关联：[AGENTS.md](../AGENTS.md) §7 范围边界、[roadmap.md](roadmap.md) P3、[ui-polish.md](ui-polish.md) §7、[adr-001](adr-001-gui-and-architecture.md) §3

---

## 1. 为什么做、以及为什么是「只读」

诉求：坤哥手上有多个 AI 会话（本仓库的自动循环、其它项目的工作区），它们反复需要
「在这堆 md 里找某段内容」。与其每个 AI 各自 `grep`，不如让 LaterMD（它已经有文件树根、
有搜索服务、有大纲、有 Git 快照）把这些能力以 MCP 工具暴露出去，一次接线处处可用。

**只读是硬边界**，理由与 P2 Git 一致（AGENTS.md §7）：LaterMD 的写路径必须有用户显式
动作与确认模态（保存、回滚都要走 UI）。让外部 AI 直接写文件等于绕过这个防线，且
AI 并发写 + 编辑器缓冲在内存 = 必然丢改。因此**第一批工具全部只读**。

---

## 2. 形态选择

| 维度 | 决策 | 理由 |
|---|---|---|
| 传输 | **stdio 主通道** + 可选 HTTP `127.0.0.1:<port>` | stdio 零端口冲突、零防火墙提示、客户端（Claude Code/Cursor）默认支持；HTTP 只作为兜底给不支持 stdio 的客户端 |
| 协议 | **JSON-RPC 2.0** + MCP 2025-06 版能力协商 | 官方协议；只实现 `tools/list` + `tools/call`，不做 resources/prompts（文档库用 tools 表达足够，少一层概念） |
| 位置 | **进程内后台线程**，应用启动即随开关起停 | 不拆独立二进制：用户装的是 GUI 应用，多一个进程就要多一套生命周期与崩溃处理；进程内还能直接复用文件树根与搜索缓存 |
| 并发 | 单连接串行（stdio 天然）；HTTP 侧单 accept 循环 | 工具调用是毫秒级检索，排队即可；不做并发模型 |
| 默认状态 | **关闭** | 默认监听任何端口都是攻击面；用户显式开启 |

---

## 3. crate 结构（守铁律二：业务逻辑不依赖 UI 框架）

```
crates/latermd-mcp/          # 新 crate,P3 时机(roadmap crate 增量表)
├── src/lib.rs               # Server: JSON-RPC 分派 + 能力协商
├── src/protocol.rs          # 请求/响应/错误结构 + 序列化(serde)
├── src/tools.rs             # Tool trait + 五个只读工具的入参/出参
├── src/transport/stdio.rs   # 行分隔 JSON(stdin/stdout)
└── src/transport/http.rs    # 阶段 ④:127.0.0.1 单连接
```

**依赖铁律**：`latermd-mcp` **不得依赖 egui/eframe**（同 `latermd-ai`、`latermd-git`）。
它依赖 `latermd-md`（大纲）与搜索的文件遍历逻辑 —— 后者目前在 `latermd-app::search`，
因此**阶段 ① 的前置动作是把搜索核心下沉**为可复用 API：

- 方案 A（推荐）：把 `crates/latermd-app/src/search.rs` 的遍历/匹配核心迁到
  `latermd-mcp` 依赖得到的位置 —— 新建 `latermd-search` crate，app 与 mcp 同消费。
  代价：一次搬迁 + 改 app 侧引用；收益：单一实现，MCP 与侧边栏搜索行为必然一致。
- 方案 B：MCP 自己实现一份遍历。代价：两套 `.gitignore` 语义与两套截断上限，必然漂移。

取 **方案 A**。

---

## 4. 工具集（v1，全部只读）

| 工具 | 入参 | 出参 | 复用 |
|---|---|---|---|
| `search_docs` | `query: string`, `case_insensitive?: bool`, `max_hits?: u32` | `{ hits: [{ path, line_no, snippet }], truncated: bool }` | `latermd-search` |
| `read_document` | `path: string`, `offset?: u32`, `limit?: u32` | `{ text, total_lines, truncated }` | 文件 IO |
| `outline` | `path: string` | `[{ level, text, line_no }]` | `latermd-md::outline` |
| `list_files` | `dir?: string`, `glob?: string`, `limit?: u32` | `{ entries: [{ path, is_dir }], truncated }` | `latermd-search`（WalkBuilder，尊重 .gitignore） |
| `git_status` | （无） | `{ entries: [{ path, kind }], truncated }` | `latermd-git::status`（只读） |

**上限沿用既有常量口径**：`search_docs` 默认 `MAX_HITS`（与侧边栏同值）、`list_files`
默认 `MAX_CHILDREN`（与文件树同值）、`git_status` 默认 `DEFAULT_STATUS_LIMIT`（与 Git
页同值）—— 与 decisions-pending #19「与既有先例同量级」的裁决一致。

**路径安全**：所有 `path` 参数先 `canonicalize`，再校验前缀落在「当前文件树根」之内；
`..`、符号链接逃逸、绝对路径越界一律 `InvalidParams`。根未设置时全部工具返回
「未设置文件树根目录」（与 AI commit message 的既有口径一致，decisions-pending #14）。

---

## 5. UI 与配置

设置对话框 MCP 分区（[ui-polish.md](ui-polish.md) §5）：

| 项 | 形态 |
|---|---|
| 启用 | 开关（默认关）；开启即起后台线程，关闭即停 |
| 传输 | stdio（常开）/ HTTP 端口（默认 `8731`，可改，`1024-65535` 校验） |
| 工具开关 | 五个工具逐个复选（最小权限：只开需要的） |
| 调用计数 | 每工具本次运行累计次数，只读展示 |
| 状态行 | 关闭 / stdio 已就绪 / 监听 127.0.0.1:8731 / 失败原因 |

配置落 `mcp.json`（与 `settings.json` / `ai.json` 同目录）。**端口与开关不算机密**，
但工具开关是权限边界，落盘与 UI 双向一致，不做隐藏。

---

## 6. 落地阶段与验收

| 阶段 | 内容 | 验收 |
|---|---|---|
| ① | 抽 `latermd-search`（方案 A）+ 新建 `latermd-mcp` 协议与 Tool trait | app 侧边栏搜索行为不变（既有测试全绿）；`latermd-mcp` 单测覆盖参数校验与错误码 |
| ② | stdio server + 进程内装配 + 五个工具 | 用 `claude mcp add` / 一个手写 JSON-RPC 客户端跑通 `tools/list` 与一次 `search_docs`；路径越界被拒 |
| ③ | 设置页开关/端口/工具开关/调用计数 | 开关起停不阻塞 UI 帧；计数随调用增长；关闭后再调用即拒 |
| ④ | HTTP 通道 | 仅本地回环可连；非 loopback 来源拒绝 |

**门禁**：vendor 未动时只跑六项（fmt / 三轮 clippy / test / doc）；新增 crate 需同步
登记 [adr-004](adr-004-technical-stack.md) 技术栈表（decisions-pending #4 的口径）。

---

## 7. 风险

| # | 风险 | 等级 | 缓解 |
|---|---|---|---|
| 1 | 搜索核心下沉破坏既有搜索行为 | 中 | 方案 A 搬迁时原测试整体随迁，先绿后接 |
| 2 | 外部 AI 高频轮询拖慢 UI | 中 | 工具在后台线程执行（与搜索同款 channel 模式），不占 UI 帧；HTTP 侧单连接串行 |
| 3 | 协议版本漂移（MCP 规范迭代快） | 中 | 只实现 `tools/*` 两个方法与固定版本字符串；不实现 resources/prompts/sampling |
| 4 | 端口冲突 / 安全软件拦截 | 低 | 默认关；端口可改；失败原因直显状态行 |
| 5 | 范围蔓延（被要求加写工具） | 高 | 本文件即边界：v1 只读，写工具需单开 ADR |

---

## 8. 与范围边界的关系

AGENTS.md §7 的 P3 深水区原列 Live Preview / 大纲预览跳转 / wikilink。MCP 不在其中，
属新增诉求，因此**以本文档登记为 P3 附加项**，排期插在 P3 之后（依赖 P1 搜索与 P2 Git
都已落地 —— 二者现已就绪，故可随时开工）。
