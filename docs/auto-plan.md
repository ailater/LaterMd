# LaterMD 自动推进计划（auto-plan）

> 本文件是**自动开发循环的唯一事实来源**：功能队列、当前状态、看护规则。
> 由每 5 分钟的看护自动化（`automation-5b29bca9`「每5分钟工作流额度看护」）驱动：
> 每个功能一个动态 workflow，完成 → 验证 push → 更新本表 → 自动开下一个，直到队列清空。
> 需要用户决策的事项一律记录到 [decisions-pending.md](decisions-pending.md)，同时自选一个最优解继续，**不等人工**。

创建：2026-09-25（用户指令：不要人工介入，先自动完善功能，决策记档自选最优，逐功能推进直到全部完成）

## 看护规则（每次触发执行）

1. `ListWorkflowRuns` 找 label 含「自动流水线」的最新非 superseded run（即当前活跃 run）。
2. running → 「巡检正常：」一行；额度类停止 → 切备用模型/等刷新（主模型 `account:bigmodel-individual-coding-plan/GLM-5.3`，备用 `account:bigmodel-start-plan/GLM-5.3-Flash`）；errored(脚本级) → 修复脚本后 Amend。
3. completed → 验证 push（脚本内有 `git push`；若 notCovered 显示未推，看护补推 `git push`）→ 把本表对应条目改为 ✅（含 run id、日期）→ 按「脚本模板」写下一个待开始功能的 workflow 脚本（规格见条目内），`CreateWorkflow(path=…)` 启动，label 命名 `自动流水线-N-<功能名>`。
4. 同一功能连续失败 3 次 → 该条目标 ❌挂起，写 decisions-pending.md，**继续下一个不卡死循环**；被挂起功能依赖的条目标 ⛔受阻。
5. 队列全部终态（✅/❌/⛔）→ 输出终局汇总（每功能一行：结果、commit、run id）+ 提醒删除看护。
6. 每次触发最多一个变更动作；绝不调用 CronCreate/CronUpdate/CronDelete。

**脚本模板**（写新脚本时照此结构，规格从条目内取）：
开头 `git checkout main && git pull`（工作基于 main）→ phase「依次实现本功能的模块并过门禁提交」（每模块：实现者 agent + fmt/clippy/test 门禁 3 轮重试 + git commit）→ phase「六项门禁复验并独立评审」（`bash vendor/egui_markdown/check.sh` + 只读独立评审员找 high/medium）→ phase「修复评审问题」（修复者 + 门禁 + commit）→ phase「更新文档推送并收尾」（roadmap 当前位置、本表状态行、decisions-pending 追加、`git push`、artifact.markdown 报告 primary）。公共约束与 persona 参考 `.zcode/workflow-drafts/P0-骨架开发流水线.dwf.ts` 的 COMMON / IMPL_PERSONA。

## 功能队列

| # | id | 功能 | 状态 | run / commit |
|---|---|---|---|---|
| 1 | p0-fixes | P0 收尾修补：跨平台 CJK 字体候选、ADR-004 登记 serde/serde_json、设置面板显示渲染后端 | ✅完成 | feat(app): CJK 字体候选表补全三平台路径,单候选失败跳下一候选;docs(adr-004): 补登 serde/serde_json 依赖两行,订正 latermd-app Cargo.toml 错误注释;feat(app): 设置菜单显示当前渲染后端(wgpu/glow) |
| 2 | p1-search | 侧边栏全文搜索：`ignore` + `grep-searcher` + `regex`，300ms 防抖 + 可取消 + 流式结果 | ✅完成 | feat(app): 仓库全文搜索核心服务(遍历+行正则+可取消流式回传);feat(app): 侧边栏搜索 UI 与跳转接线(300ms 去抖 + 流式结果 + 点击跳行) |
| 3 | p1-ai-base | latermd-ai 基础：provider trait（OpenAI/Anthropic/Ollama 兼容）+ `heal()` 流式接入 + 稳定 widget id；**mock provider 交付**（真实 key 见 decisions-pending #3） | ✅完成 | feat(ai): 新建 latermd-ai 核心 crate(provider trait + MockProvider + OpenAI SSE 解析 + ureq 阻塞 HTTP);feat(app): 接入 AI Mock 流式续写(菜单/Message 归约/防重入),预览启用 heal 并复测流式 bench |
| 4 | p1-ai-links | `ai://` 链接协议 + AI 指令块（LinkHandler 五级扩展点落地），依赖 #3 | ⏳待开始 | — |
| 5 | p1-ai-tools | AI commit message + AI 摘要大纲（基于 #3 的 provider，mock 可用） | ⏳待开始 | — |
| 6 | p2-git | latermd-git 只读集成：状态、历史、diff、blame、回滚 + 文件树 M/A/U/? 标记 | ⏳待开始 | — |
| 7 | p2-creds | 凭据管理三平台封装（Credential Manager / Keychain / libsecret） | ⏳待开始 | — |
| 8 | p2b-theme | 皮肤批次 B：三态切换（亮/暗/跟随系统）+ 自定义皮肤文件（RON 导出至 themes/）+ 视觉打磨 | ⏳待开始 | — |
| 9 | p3-live-preview | Live Preview：块级 caret 路由 + 聚焦块裸源码；**共用同一 rope buffer 与 undo 栈**（roadmap 铁律） | ⏳待开始 | — |
| 10 | p3-nav | 大纲预览跳转（复用 section_to_token 映射）+ `[[wikilink]]` 双向链接（LinkHandler） | ⏳待开始 | — |

状态图例：⏳待开始 → 🔄进行中 → ✅完成 / ❌挂起（3 次失败）/ ⛔受阻（依赖挂起）。

## 各条目规格（写脚本时取用）

**#1 p0-fixes**（三模块）：
- fonts：`crates/latermd-app/src/fonts.rs` 的 CANDIDATES 补 Windows（`C:\Windows\Fonts\msyh.ttc`、`simhei.ttf`）与 macOS（`PingFang.ttc`、`Hiragino Sans GB.ttc`）候选路径，运行时 exists 探测；Linux 保留现状；附结构单测。
- adr：`docs/adr-004-technical-stack.md` 技术栈表登记 serde/serde_json（主题持久化用），订正 latermd-app Cargo.toml 里「criterion 传递依赖」的错误注释。
- backend：检查 AGENTS.md §5「Settings 面板显示当前渲染后端（wgpu/glow，LATERMD_RENDERER 逃生口）」是否已实现，缺则补。

**#2 p1-search**：新建搜索服务（后台线程 `ignore::WalkBuilder` + `grep-searcher` + `regex`，channel 回传不碰 UI 状态）；侧边栏 Search tab：输入框 300ms 防抖、可取消（新搜索丢弃旧结果）、流式追加结果列表（路径+行号+摘要），点击结果打开文件并跳转行。依赖已在 ADR-004（ignore 已用于文件树，grep-searcher/regex 需登记 ADR——脚本内处理）。

**#3 p1-ai-base**：新建 `crates/latermd-ai`（P1 时机，roadmap crate 增量表）：`AiProvider` trait（`stream_complete(prompt) -> impl Stream<Chunk>`，OpenAI/Anthropic/Ollama 三种请求格式的 adapter，**不内置任何 key**，从环境变量/设置读）+ `MockProvider`（按 100ms 吐 chunk，用真实文本）。app 侧接入 vendored `heal()`：mock 流式喂给预览，验证 widget id 稳定（不含 content.len()）与增量高亮不失效。bench 跑 `bench_render_scroll_code_streaming` 记录数字。

**#4 p1-ai-links**：vendored `LinkHandler` 落地 `ai://` 协议（link_style 区分样式、click 拦截产生 Message）；AI 指令块（```ai fenced 块 → is_block_widget/block_widget 渲染为指令卡片）。vendor 改动按 §6 三类拆 commit 并登记 vendor/README.md。

**#5 p1-ai-tools**：AI commit message（读取 staged diff 摘要请求 provider 生成，按钮插入提交框——app 侧暂以 mock 文案走通链路）；AI 摘要大纲（全文喂 provider 生成大纲插入文档或侧栏，mock 走通）。

**#6 p2-git**：新建 `crates/latermd-git`（git2，只读）：状态列表、log 历史、diff（HEAD vs workdir，按文件）、blame（当前文件行级）、回滚（checkout 单文件，唯一写操作，需确认模态）。侧边栏文件树加 M/A/U/? 角标。

**#7 p2-creds**：凭据存取 trait + 三平台实现（Windows Credential Manager via `keyring` crate 或 winapi、macOS Keychain、Linux libsecret；选 `keyring` 统一封装最省——ADR 登记）。供 #3 的 API key 存取用。

**#8 p2b-theme**：Theme 加三态（亮/暗/跟随系统，`dark-light` crate，Linux 失灵回退手动）；Theme serde 导出 RON 至用户配置目录 `themes/`，启动扫描可加载；视觉打磨（间距/圆角 token 统一、滚动条、hover 态、编辑器行距）。

**#9 p3-live-preview**：编辑器加 `render_mode` 标志（一个编辑器两种模式，共用 rope buffer/undo）；光标所在 block 用 source_span 显示源码，其余走富渲染；块间 caret 路由（↑/↓ 跨 block、Home/End）。参考 vendor README 的 render_token_range。

**#10 p3-nav**：大纲点击滚动预览到对应 section（section_to_token 映射 + scroll_to_rect）；`[[wikilink]]` 解析（latermd-md 层加语法或预处理）+ LinkHandler 点击打开文件树中同名 md。

## 人工待办（自动循环不做）

- IME 真机实测（Win11 微软拼音 / macOS 简体拼音）——M0 挂账项
- Win/mac wgpu 真机启动验证
- 三平台打包真机验收：`feature/p0-packaging` 分支（cargo-dist + universal2 dmg + cask 模板已就绪）等待人工验收合并，**自动循环不碰该分支**（见 decisions-pending #1）
- AI provider 真实 API key 配置
- 视觉终审与发布（打 tag 触发 Release）
