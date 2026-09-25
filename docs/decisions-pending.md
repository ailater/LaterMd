# 待用户决策记录（decisions-pending）

> 自动开发循环遇到「本该问用户」的岔路口时，在这里登记：**岔路是什么、自动选了什么、为什么、想改怎么改**。
> 选择由循环自行做出并继续执行，不阻塞；用户事后翻此文件，按「如何改」一节操作即可推翻。
> 编号 #6 为当前阻塞项，需用户裁决。

## #10 heal() 的作用时机：逐块(后台线程) vs 整篇(渲染帧)（2026-09-25）

- **岔路**：AI 流式接线 ask 要求「后台线程每块先过 vendored heal()，经 mpsc 回 UI，delta 追加进编辑器 rope」。但 `heal(s)` 的语义是给**残缺文本前缀**补闭合标记（`vendor/egui_markdown/src/parser.rs:45`：`heal("```rust\nlet x = 1;")` → 追加闭合 fence、`heal("**bold text")` → 追加 `**`）。MockProvider 按固定 20 字符切块，块边界会切在代码 fence/加粗中间：若把逐块 healed 文本追加进 rope，闭合标记会被**永久写进文档**，且下一块拼在闭合 fence 之后产生非法残文（例：块尾 `fn invalidate(cache: &mut C` 被补成 `…C\n```` ，下一块 `ache, doc_id…` 紧跟其后再开一个 fence）。
- **自动选择**：heal 移到**渲染帧**、作用于**整篇快照**——预览 `MarkdownLabel` 开 `.heal(true)`（`crates/latermd-app/src/ui/preview.rs`），vendored 层在 parse 前对全文调 `parser::heal`（`label.rs` render 的既定钩子，docstring 即「Useful for streaming LLM output」）；编辑器 rope 只收原始 delta，文档内容不被污染。这与 AGENTS.md §6.5「让 LLM 流式输出的每一帧语法合法」一致——每一帧 = 每次渲染的全文快照。预览对完整文档 heal 是恒等变换（`Cow::Borrowed` 原样返回），非流式场景行为不变。
- **如何改**：若确实要逐块 heal（例如想把「 healed 帧」单独喂给某个纯预览通道、不进编辑器），在 `crates/latermd-app/src/ai.rs` 的 `poll()` 里对 delta 调 `egui_markdown::heal` 并另开一条不落盘的预览通道即可；只要别把 healed 文本写进 `EditorBuffer`。

## #9 流式失败信号与 OpenAI adapter 默认值（2026-09-25）

- **岔路**：`latermd-ai` 的 `Chunk` 按 ask 固定为 `{ delta, done }` 两字段，但流式请求失败（HTTP 非 2xx / 连接中断 / 读超时）没有天然的信号位；另外 OpenAI adapter 的默认端点、模型名与是否引入额外环境变量，ask 未规定。
- **自动选择**：约定「`done == true` 且 `delta` 非空 = 流失败，`delta` 是面向用户的错误描述（不写入文档）；`done == true` 且 `delta` 为空 = 成功结束」，见 `crates/latermd-ai/src/lib.rs` 的 `Chunk` 文档。默认端点 `https://api.openai.com/v1`、默认模型 `gpt-4o-mini`，可分别用 `LATERMD_AI_BASE_URL`、`LATERMD_AI_MODEL` 覆盖（key 仍只有 `LATERMD_AI_API_KEY`，decisions-pending #3 不变）。
- **如何改**：若希望失败信号更显式（如 `Chunk` 加 `error` 字段或改 enum），改动点集中在 `latermd-ai` 的 `Chunk` 定义与 `openai.rs::run`，消费方尚只有 mock 联调链路，无迁移负担；默认模型/端点改 `OpenAiProvider` 两个 `DEFAULT_*` 常量即可。

## #8 搜索去抖到点发起从 `ui::sidebar` 挪进归约侧（2026-09-25）

- **岔路**：修复「清空搜索输入 / 输入后切走页签后 `debounce_due` 残留过期时刻，`layout.rs` 每帧 `request_repaint_after(ZERO)` 满帧空转」时，评审给了两个薄修：①去掉 `ui::sidebar` 到点判断里的非空输入条件；②`layout.rs` 对已过期的 due 不再要帧。①只修「清空输入」主路径，「输入后切到 Files/Outline 页签」路径 `search_panel` 不渲染、无人清计时，依旧空转；②会打断接力最后一环——到点帧 reduce 先于 ui 执行，reduce 见 remaining==0 不要帧后，同帧 `ui::sidebar` 发出的 `SearchRequested` 滞留 outbox，无下一帧 apply，表现为「输入完不动鼠标搜索永不发起」。
- **自动选择**：把到点判断整体挪进 `layout.rs` 的 `reduce`（每帧必跑、不看页签可见性），到点当帧 `apply(SearchRequested)` → `SearchState::start` 入口清计时，过期 due 活不过一帧；重绘驱动与到点判断同处一处，两类残留（空输入 / 切页签）一并消除。副产品：输入后 300ms 内切走页签也照常发起，切回来即见结果（比「切回 Search 页才自愈」更符合直觉）。
- **如何改**：若更在意「只在 Search 页可见时才发起」，把 reduce 里的到点块移回 `ui::sidebar::search_panel` 并同时采纳②之外的方案（例如 reduce 里对过期 due 保留一次要帧兜底）；三个测试锚点在 `layout.rs`（`search_debounce_fires_in_reduce_even_when_tab_switched_away`、`search_debounce_due_cleared_for_empty_query_without_repaint_loop`）。

## #7 搜索核心不用 `grep-regex` 桥接（2026-09-25）

- **岔路**：roadmap 阶段 3 搜索条目的组件清单写作「`grep_searcher::Searcher` + `regex`」，但 `grep_searcher::Searcher::search_*` 全系 API 要求 `grep_matcher::Matcher` 实参，`regex::Regex` 并未实现该 trait，二者**无法直连**。要么补引 `grep-regex`（ripgrep 官方桥，连带 `grep-matcher`），要么改用同 crate 的 `grep_searcher::LineIter` 做行迭代、`regex` 直接匹配。
- **自动选择**：`LineIter`（`regex::bytes` 变体）+ 直接匹配，P1 搜索核心已按此落地（`crates/latermd-app/src/search.rs`）。不引 `grep-regex` 的理由：它带来的只是 trait 适配与流式读取，而 md 单文件整读进内存完全可行（实现里加了 16MB 单文件上限防病态大文件），大小写开关由 `RegexBuilder::case_insensitive` 一行承接；少一个清单外依赖比「与 ripgrep 同构」更有价值（#4 口径）。
- **如何改**：若 P1 搜索面板需要 multiline 正则或超大文件流式匹配，改引 `grep-regex` + `grep-matcher` 并在 ADR-004 补登，替换 `src/search.rs` 的行循环即可；对外三原语（发起 / 接收 / 取消）与事件模型不变。

## #6 双会话并发冲突（已裁决，2026-09-25）

- **事实**：2026-09-25 上午，本自动循环与另一活跃会话（UI 设计文档线）共享同一工作目录，互相踩踏致第一棒四次卡在 `git checkout main`，被主动停止（stop_reason=model，可恢复，无半截污染）。
- **裁决**：用户选择**选项 1（继续循环）**——已人工解决冲突（`d5de607` 取 stash 侧）、合并 origin/main 进 feature/kun、提交循环文档（`6b2ddc0`，已 cherry-pick 到 main 为 `8a500f0`）、清空 stash。
- **遗留提醒**：若其他会话今后仍需在此工作区工作，建议改用选项 2（`git worktree add ../LaterMD-auto main` + 脚本改造），循环脚本开头的两步 stash 防御只是兜底不是根治。

## #1 打包工作的合并（已解决）

- **原岔路**：打包工作在 `feature/p0-packaging` 分支，自动循环是否代为合并。
- **结果**：另一会话已走正规 PR 流程合并——PR #7（`feature/p0-packaging`：cargo-dist、universal2 dmg、release workflow、cask 模板）与 PR #8（`feature/m0-perf-bench`：长文档 bench）均已合入 main（`f822b61`）。剩余真机验收（打 tag 看 Release、brew 装机）仍属人工。

## #2 直推 main 豁免（2026-09-25 中午起已被现实推翻，改为 PR 自合并通道）

- **原自动选择**：循环直推 origin/main（用户晨间指令）。
- **新事实**：远端 main 已开启分支保护（require PR，GH013 拒绝直推）。
- **现行流程（PR #13 验证可行）**：每棒完成后 `git push origin HEAD:refs/heads/auto/<功能名>` → `gh pr create --head auto/<功能名> --base main` → `gh pr merge <N> --merge --delete-branch` → 本地 `git checkout main && git pull`。满足保护规则且无需人工；若仓库后续加 required review 导致自合并失败，则退化为「推分支 + 提示人工合并」。
- **注意**：rebase 远端新提交时文档冲突（roadmap/README 修订表）按「两边行都保留」合并。

## #3 AI provider 的 API key

- **岔路**：P1 AI 功能（队列 #3-#5）需要真实模型端点与 key 才能端到端。
- **自动选择**：以 MockProvider（100ms/chunk 流式）交付全部链路与 UI；provider trait 预留 OpenAI/Anthropic/Ollama adapter，key 从 `LATERMD_AI_API_KEY` 或凭据管理读取，**代码不硬编码任何 key**。
- **如何改**：设置界面填 key（或 `export LATERMD_AI_API_KEY=...`），选 provider 后重启即用真实模型。

## #4 依赖新增的登记口径

- **岔路**：循环会给仓库引入 ADR-004 清单外依赖（grep-searcher、dark-light、keyring 等）。
- **自动选择**：每引入一个，同步在 `docs/adr-004-technical-stack.md` 表内登记（版本+用途+notes），视作 ADR 滚动修订。
- **如何改**：若某个依赖不认可，revert 对应 commit，登记行随代码一起回退。

## #5 打包分支 WIP 的贮藏（现状已简化）

- **岔路**：切换 main 时工作区残留未提交改动，阻塞 checkout。
- **自动选择**：stash 无损贮藏，不丢、不代提交。
- **现状**：打包工作已随 PR #7 合入 main；工作区现存 README.md / docs/README.md / docs/roadmap.md 修改与 docs/distribution.md 属 UI 设计线会话恢复的 WIP，归它处置，本循环不再 stash（见 #6）。stash 栈若仍有 `auto-cycle:` 条目，恢复前先 `git stash show -p` 与 main 对比，无增量直接 drop。
