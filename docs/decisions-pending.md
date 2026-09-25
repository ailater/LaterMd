# 待用户决策记录（decisions-pending）

> 自动开发循环遇到「本该问用户」的岔路口时，在这里登记：**岔路是什么、自动选了什么、为什么、想改怎么改**。
> 选择由循环自行做出并继续执行，不阻塞；用户事后翻此文件，按「如何改」一节操作即可推翻。
> 编号 #6 为当前阻塞项，需用户裁决。

## #19 git status 的条数上限与超限文件的行为（2026-09-25）

- **岔路**：独立评审指出 `latermd_git::status` 无条数上限（`recurse_untracked_dirs` 全量展开），叠加同步跑在 UI 线程的 3s 轮询与侧边栏每帧全量渲染，超大仓库会卡帧——log（50 条）与 diff（64KB）都有上限，唯独 status 没有。加多少、超限文件的行为（角标/选中/回滚）怎么定未指定。
- **自动选择**：上限 **500**（`latermd_git::DEFAULT_STATUS_LIMIT`），与文件树 `MAX_CHILDREN`、搜索 `MAX_HITS` 两个既有先例同量级；`status(root, limit)` 返回 `StatusSnapshot { entries, truncated }`，**先按路径排序再截断**（保留字典序最小的前 500 条，保证确定性）；Git 页改动列表尾部渲染「…还有 N 项未显示」（与文件树同款提示行）。超限文件的降级：无文件树角标、不可选中/回滚（select/request_checkout 的「列表外路径忽略」防御天然覆盖）——与文件树截断、搜索 MAX_HITS 同语义。注意：libgit2 的 statuses 遍历本身无法提前截断（StatusOptions 无 limit），本上限消除的是「Vec 无界 + 每帧全量渲染」两项；遍历成本仍属 #17 已登记的「掉帧再挪线程」取舍。
- **如何改**：嫌 500 太小改 `DEFAULT_STATUS_LIMIT` 一个常量（调用方 `git_panel.rs` 自动跟随）；要「显示全部」，给 Git 页加展开交互并让 `status` 支持分页或提高上限；要消除遍历成本，把 `recurse_untracked_dirs` 关掉（未跟踪目录只报目录一条，快得多，但文件树逐文件打标失效）或按 #17 的「如何改」挪后台线程。

## #18 回滚目标恰是编辑器当前文档时，dirty 缓冲的处置（2026-09-25）

- **岔路**：独立评审指出回滚（checkout 单文件）若目标正是编辑器当前打开的文档，归约后编辑器不重载、无提示——dirty 场景一次 Ctrl+S 就把被丢弃的改动静默写回（反转回滚）；非 dirty 场景编辑器显示与磁盘不一致。修法有两派：①回滚后无条件重载编辑器（强一致，但 dirty 时静默丢掉未保存稿）；②分 dirty 分流。
- **自动选择**：**②分 dirty 分流**（`State::after_git_checkout`）：目标非当前文档不触碰编辑器；是当前文档且非 dirty → 重读磁盘换入缓冲（预览同帧联动，编辑器与磁盘重新一致）；是当前文档且 dirty → **保留未保存稿**（静默丢稿的代价大于不一致，与 `unsaved_guard` 的既有哲学一致），提示行明示「已回滚 X：编辑器里未保存的修改仍保留，保存(Ctrl+S)会把它们写回」。确认模态同步加针对性警示（`checkout_dialog` 的 `checkout_extra_warning`）：目标在编辑器中打开时按 dirty 显式告知上述行为，不再只有通用不可逆警示。
- **如何改**：要①的强一致语义，把 `after_git_checkout` 的 dirty 分支改成同样调 `open_from(file)`（并在 `checkout_extra_warning` 的 dirty 文案里说明将丢弃编辑器修改）；要更保守的「dirty 时拒绝回滚」（像 unsaved_guard 那样拦下），在 `request_checkout` 前置检查并落提示行。

## #17 Git UI 接驳的刷新机制、仓库根定位与确认模态形态（2026-09-25）

- **岔路**：把 latermd-git 接进侧边栏时任务留了三处自由度。①「每 N 秒或触发时刷新」的 N 未定，且同步归约执行还是后台线程未定（`git status` 大仓库冷缓存可能上百毫秒，卡帧风险真实存在）；②#16 ③ 已定 latermd-git API 层用 `Repository::open` 严格仓库根，但 UI 的文件树根常是**仓库子目录**（比如选了 `docs/` 当根），以哪个目录调 status/diff/log 没定；③「确认模态」在 egui 0.36 没有内建阻塞模态层，用什么形态承载。
- **自动选择**：①**N=3 秒，同步在归约里执行**（`crates/latermd-app/src/git_panel.rs::REFRESH_INTERVAL`）：与 `git_diff.rs` 的既有口径一致（本地 git 读是毫秒级，不上后台线程），到点由 `ui::layout::reduce` 触发并 `request_repaint_after` 要帧；触发式刷新 = 换根 / 切到 Git 页 / 回滚完成；**降级（非 git 目录）即停轮询**，重探由换根/切页签触发——保证 egui 空闲收敛（search 去抖测试守护的不变量），零成本挂着的失败探测没有价值。②新增 `latermd_git::discover`（`Repository::discover` 向上探测，裸仓库显式 Err）：crate 其余 API 的「严格仓库根」口径不动，UI 接驳层先 discover 把文件树根换算成仓库根；状态条目仍记「相对仓库根」路径，角标拼成绝对路径与文件树条目匹配。③确认模态用**非阻塞 `egui::Window` 浮窗 + 红色警示文案「未提交的改动将被丢弃，此操作不可撤销」**（与 AI commit 建议浮窗同模式）：checkout 只在「回滚」按钮点击后的消息归约里执行，浮窗本身零 git 调用。
- **如何改**：嫌 3s 太钝/太勤，改 `REFRESH_INTERVAL` 一个常量；大仓库实测掉帧，把 `GitPanelState::refresh` 的两次 git 读挪后台线程（对 UI 的接口不变，参照 search 的代际号取消模式）；要收紧回「文件树根必须是仓库根」，删 `latermd_git::discover` 并让 `GitPanelState::refresh` 直接以文件树根调 status（非根目录会走降级提示）；要真阻塞式模态，等 egui 内建 modal 层（0.36 无）或自绘全屏遮罩 Area。

## #16 latermd-git 的三个落地口径：U 的语义、git2 features、仓库根定位（2026-09-25）

- **岔路**：P2 首个 Git crate 落地时任务留了三处歧义。①状态码集合写作 `M|A|U|D|?`，U 是 unmerged（git CLI short format 语义）还是 untracked（VS Code 装饰字母语义）——若 U=untracked 则 `?` 无含义。②ADR-004 登记 git2 0.21.0 的组合是 `vendored-libgit2 + vendored-openssl`，但 vendored-openssl 会拉 openssl-src 全量编译（三平台 CI 各多数分钟），而它的唯一用途是 https 传输。③API 以仓库路径为参数：`Repository::open`（严格根）还是 `Repository::discover`（向上层搜 `.git`）。
- **自动选择**：①**U=unmerged（合并冲突），?=untracked**，按 git CLI `--short` 语义（`crates/latermd-git/src/lib.rs::StatusKind`），与 roadmap「文件树 Git 标记 M/A/U/?」并排五码自洽；②git2 取 **`default-features = false, features = ["vendored-libgit2"]`**（版本 0.21 与 ADR-004 一致）：P2 明确只读、无 fetch/push/pull，ssh/https 传输层整层用不上，关掉后零 openssl 面（不依赖系统包、不编译 openssl-src）；若将来做 remote 再加 `vendored-openssl` 即可；③**`Repository::open` 严格仓库根**，不向上搜——与 #14 app 侧「不向上搜 `.git`」的既有口径一致，非 git 目录一律 `Err` 交给 UI 降级成提示。
- **已知并接受的边界**：status 里非 UTF-8 文件名不出现（libgit2 的 `entry.path()` 返回 `Option<&str>`，非 UTF-8 时为 None，极罕见）；blame 基于 HEAD 提交内容，工作区未提交的行不参与行级归属（libgit2 限制，doc 已注明）；`checkout_file` 的 path 走 git pathspec 语义（与 `git checkout -- <path>` 一致，含 glob 元字符的文件名理论上可被通配匹配）。
- **如何改**：要改 U=untracked，改 `lib.rs::status_kind` 的优先级映射一处（untracked 同时映射 U 与 `?` 的需求不存在，五码本来就单字母）；要恢复 ADR 原样的 openssl 组合，把 crate Cargo.toml 的 features 改回 `["vendored-libgit2", "vendored-openssl"]`（须同时去掉 `default-features = false`，否则 https feature 仍关着）；要支持从子目录自动定位仓库根，把 `open_repo` 的 `Repository::open` 换成 `Repository::discover`，但 status/diff 的相对路径语义需随之在 UI 侧重排。

## #15 AI 摘要的插入形态、引用块前缀来源与 Mock 请求识别（2026-09-25）

- **岔路**：任务把展示形态留成二选一——「以引用块形式插入文档末尾」或「展示给用户可选插入」。另外 prompt 输出要求固定为「每条一行中文，以 '- ' 开头」，而最终插入形态是引用块（`> - …` 行），`> ` 前缀由谁加上、Mock provider 在共用 `stream_complete` 通道里如何区分摘要请求与续写请求，都需要定口径。
- **自动选择**：**直接插入文档末尾**（`crates/latermd-app/src/state.rs::request_summary`）。理由：摘要流式落文档与续写同语义，天然复用 `AiChunk` 追加通道与防重入；浮窗形态走不了流式追加（commit 选浮窗是因为 subject 是「建议」，摘要是要写进文档的内容）。`> ` 前缀由 **Mock 替身直接产出最终文档形态**（`crates/latermd-ai/src/mock.rs::mock_summary` 输出 `> - …` 行序列），prompt 指令保持任务原口径不改；真实 key 接入时在适配层把模型输出的 `- ` 行包成 `> - `（与 commit 的真实 provider TODO 同批，见 `request_commit_message` 的 TODO 注释）。Mock 请求识别用**摘要指令头前缀嗅探**（`stream_complete` 里 `starts_with(SUMMARY_INSTRUCTIONS)`，与现有「按 prompt 关键词选脚本」同构；真实 provider 无此问题，模型自己读指令）。
- **已知并接受的边界**：①摘要节定位按「二级标题 + 文本精确等于 `AI 摘要`」（`latermd_md::heading_section_span`），用户改层级/改名后的旧节不清理（保守匹配，防误删手写内容）；②流失败时旧节已删、新标题已插、要点可能半截——与续写流「失败留半截正文」同语义，演示期 Mock 不产生失败块；③摘要节被用户挪到文档中间且其后还有内容时，删节后正文与下一节间保留一个换行（合法 Markdown，源码视觉紧凑）。
- **如何改**：要改浮窗形态，在 `State` 加 `ai_summary_suggestion: Option<String>` 并把 `request_summary` 改成收流进缓冲区外的暂存（AiChunk 归约需按流类型分流）；要匹配用户变体的旧节，放宽 `heading_section_span` 的层级参数或做模糊文本匹配；要让真实 provider 输出自动加 `> `，在接入 `OpenAiProvider` 时于 app 侧对摘要流的 chunk 做行级包装（需在 `AiState` 加当前流类型标志）。

## #14 AI commit message 的仓库定位与浮窗/复制口径（2026-09-25）

- **岔路**：菜单「AI: 生成 commit message」要读「当前仓库」的未提交改动，但 LaterMD 没有「当前仓库」的概念——文档可以不在任何 git 仓库里，文件树根也可以是任意目录，还可以向上搜 `.git` 找仓库根。另外 ask 把展示（状态栏 vs 对话框）与复制（按钮 vs 自动写剪贴板）留成二选一。
- **自动选择**：仓库定位取**当前文档所在目录，退文件树根目录**，两者皆无则提示「先保存文档或设置文件树根目录」；不在仓库/无 git 时 `git diff` 的 stderr 直接落提示行（`crates/latermd-app/src/state.rs::request_commit_message`）。不向上搜 `.git` 找根：`git diff` 在仓库子目录里跑也返回全仓改动，先找根纯属多余。展示用**浮窗对话框**（subject 要整行可读，状态栏 notice 行是错误专用、红色语义不符）；复制用**显式「复制」按钮**（`Context::copy_text`），不自动写剪贴板——未经用户动作覆盖系统剪贴板会冲掉用户正在搬运的内容。生成路径演示期为 `MockProvider::mock_commit_subject` 同步合成（流式脚本对 commit 场景不适用），真实 key 接入后改走 `OpenAiProvider` 低温度补全取首行（同函数内的 TODO）。
- **如何改**：要支持显式指定仓库（如设置面板里选仓库根），改 `request_commit_message` 的目录解析一处即可；要改自动复制，在 `Message::AiCommitSuggestion` 归约里补 `Context::copy_text(subject)`（消息已带 subject，归约侧拿得到 ctx）；要让建议随换文档消失，在 `State::load_document` 里顺手清 `ai_commit_suggestion`（本轮刻意不清：建议是仓库级派生物，不是文档的）。

## #13 ```ai 指令卡状态行的键控口径（2026-09-25）

- **岔路**：指令卡状态行要求「未执行 / 进行中 / 已完成」三态，需要回答「哪张卡算进行中/已完成」。可选：①按卡片指令文本与最近一次发起的 prompt 匹配（`AiState::last_prompt`）；②按块在文档中的序号维护每卡状态表。
- **自动选择**：①（`crates/latermd-app/src/ui/preview.rs::AiLinkHandler::card_status`）。理由：防重入保证同时至多一个流，「哪张卡发起」由 prompt 文本即足以判定；序号表在用户增删块时会整体错位，还要处理失效清理；文本匹配零新增结构。已知并接受的简化：**两卡片指令文本完全相同则状态同亮**；指令文本被编辑后状态回「未执行」（文本变了=另一条指令，语义自洽）。菜单入口（`AiStart`）的 prompt 是文档尾部拼装文本，不会与任何指令文本相等，菜单流不点亮卡片。
- **失效时机**：`last_prompt` 只在两处清空——流失败（`Message::AiFailed` 归约，失败不算完成）与换文档（`State::load_document`，卡片是文档的派生物）；`AiDone` 后保留，让「已完成」可见。
- **如何改**：要按序号键控（同文卡片状态独立），在 `PreviewState` 加 `card_status: HashMap<usize, AiCardStatus>` 并把 `block_code_widget` 的 `index` 传进消息，即可替换匹配逻辑；消息归约与 vendored 扩展点无需动。

## #12 ```ai 指令块走最小 vendor 改动（代码块级 block widget 扩展点）（2026-09-25）

- **岔路**：任务优先「只用 app 侧扩展点，不动 vendor」。实测 vendored `LinkHandler::is_block_widget`/`block_widget`（`vendor/egui_markdown/src/link.rs`）只作用于 **`Token::Link` 的 href**（判定点 `layout.rs` `append_link_to_job`/`needs_segmentation`/`build_layout`），**够不到围栏代码块**——roadmap 阶段 3 写的「`.is_block_widget()` → `.block_widget()`」对 ```ai 围栏不成立。app 侧唯一代码块扩展点是 `code_block_buttons` 头部 overlay 回调（回调签名 `(ui, text, lang)`，无块序号/span），画不出「卡片 + 状态行」，也拿不到稳定块身份（AGENTS §6.7 的 id 稳定性无从谈起）。
- **自动选择**：给 vendored `LinkHandler` 加**代码块级 block widget** 两方法（`is_block_code_widget(language)` / `block_code_widget(ui, text, language)`，按 info string 判定），与链接 block widget 同构：命中即 segment break，在 `render_token_range` 独立渲染；`needs_segmentation` / `build_layout` / `render_token_range` 三处按上游既有「必须同步」约定同步改。类别 **①上游可合**（通用能力、带 tests/block_code_widget.rs，可 cherry-pick 提上游 PR），登记见 `vendor/README.md` 提交级登记表与 `vendor/egui_markdown/README.md` 差异表 #7。
- **如何改**：若不认可动 vendor，revert 该 ① 类 commit 并把 app 侧退到 `code_block_buttons` overlay 形态（功能降级：状态行并入代码块头、卡片视觉消失、多卡身份按内容 hash 近似）——代价已实测如上，不建议。

## #11 `ai://` 链接协议语义与 prompt 编解码口径（2026-09-25）

- **岔路**：roadmap 阶段 3 对「ai:// 链接协议」只写了「`.link_style()` + `.click()` 拦截」的实现方式，协议本体没有定稿——已实现动作是哪个、未实现动作点了怎么办、prompt 怎么编码、`+` 算不算空格，都得有个说法才能写测试。另实测发现 vendored 的 `LinkStyle.underline` 字段（`vendor/egui_markdown/src/link.rs:86`）当前**没有任何读取点**。
- **定稿**：`ai://write?prompt=<urlencoded 提示词>` 触发 Mock 流式续写，prompt **原样透传** provider（不拼文档尾部——链接作者写的就是完整指令；续写仍落在当前文档末尾，防重入与菜单「AI 续写」同一入口 `start_ai_stream_with_prompt`）。其余 `ai://` 动作（`ai://summarize` 等）**识别但不拦截成执行**：点击提示「未实现的 AI 动作：<action>」。`ai://write` 缺 prompt 参数、prompt 为空、坏 `%` 序列、非 UTF-8 字节，均提示且不执行。解码只用严格 `%XX`（`percent-encoding` 2.3.2，坏序列自行校验补严——该 crate 默认原样放行），**`+` 不当空格**：markdown 链接里作者本就该用 `%20`。非 `ai://` 前缀完全不拦截，走 vendored 默认 `open_url`（系统浏览器）。
- **证据**：vendored `layout.rs:144`（`link_style().color` 决定链接**文字色**）、`layout.rs:197`（hover 下划线对**全部**链接无条件绘制，颜色取 `link_style().color`）、`label.rs:1165`（`click` 返回 true 则跳过 `open_url`）。app 侧 `LinkStyle { color, underline: true }` 里 `underline` 是**声明意图**——vendored 层没人读它，样式区分实际由颜色承担；ai:// 链接取紫罗兰色（明暗主题两档），与默认 `hyperlink_color` 区分。
- **如何改**：新增动作或让 `+` 当空格，改 `crates/latermd-app/src/ai_link.rs::parse` 一处（消息载荷 `Message::AiLinkClicked { prompt: Result<String, String> }` 不变，`Err` 文案在 parse 里拼）；要让 `underline` 字段真正生效需改 vendored 层（①上游可合类），本轮按「优先只用 app 侧扩展点」未动 vendor。

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
