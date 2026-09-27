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

> **排队顺序（2026-09-27 坤哥指令「根据文档的最近规划，图片，emoji，ui等功能，添加新的流水线workflow」插队）**：
> **#26 图片框与图床 → #27 UI 现代化(U0/U1/U3) → #28 Emoji 面板 → #14 → #15 → #16 → #17 …**。
> 三条均已预写脚本于 `.zcode/workflow-drafts/`（`自动流水线-26-图片框与图床.dwf.ts` /
> `自动流水线-27-UI现代化.dwf.ts` / `自动流水线-28-emoji面板.dwf.ts`），看护直接 `CreateWorkflow(path=…)` 开棒，不必现写脚本。
> #26 开棒前提：并行线 A 段 WIP 收口（工作区干净）；脚本已内置 A 段三态判定——已在 main 则跳过、在途未合入则整棒中止待重开、缺失且无在途迹象才补实现。

| # | id | 功能 | 状态 | run / commit |
|---|---|---|---|---|
| 1 | p0-fixes | P0 收尾修补：跨平台 CJK 字体候选、ADR-004 登记 serde/serde_json、设置面板显示渲染后端 | ✅完成 | feat(app): CJK 字体候选表补全三平台路径,单候选失败跳下一候选;docs(adr-004): 补登 serde/serde_json 依赖两行,订正 latermd-app Cargo.toml 错误注释;feat(app): 设置菜单显示当前渲染后端(wgpu/glow) |
| 2 | p1-search | 侧边栏全文搜索：`ignore` + `grep-searcher` + `regex`，300ms 防抖 + 可取消 + 流式结果 | ✅完成 | feat(app): 仓库全文搜索核心服务(遍历+行正则+可取消流式回传);feat(app): 侧边栏搜索 UI 与跳转接线(300ms 去抖 + 流式结果 + 点击跳行) |
| 3 | p1-ai-base | latermd-ai 基础：provider trait（OpenAI/Anthropic/Ollama 兼容）+ `heal()` 流式接入 + 稳定 widget id；**mock provider 交付**（真实 key 见 decisions-pending #3） | ✅完成 | feat(ai): 新建 latermd-ai 核心 crate(provider trait + MockProvider + OpenAI SSE 解析 + ureq 阻塞 HTTP);feat(app): 接入 AI Mock 流式续写(菜单/Message 归约/防重入),预览启用 heal 并复测流式 bench |
| 4 | p1-ai-links | `ai://` 链接协议 + AI 指令块（LinkHandler 五级扩展点落地），依赖 #3 | ✅完成 | feat(app): ai:// 链接协议——LinkHandler 样式区分与点击拦截触发 Mock 流式续写;feat(app): ```ai 指令卡——代码块级 block widget 渲染指令卡,执行走 ai:// 同通道 |
| 5 | p1-ai-tools | AI commit message + AI 摘要大纲（基于 #3 的 provider，mock 可用） | ✅完成 | feat(app): AI 生成 commit message——diff 采集、prompt 模板、Mock 合成与建议浮窗;feat(app): AI 生成摘要——全文喂 provider、引用块流式插入文档末尾、旧摘要节自动清理 |
| 6 | p2-git | latermd-git 只读集成：状态、历史、diff、blame、回滚 + 文件树 M/A/U/? 标记 | ✅完成 | feat(git): 新建 latermd-git——git2 只读封装(status/log/diff/blame)与确认式单文件回滚;feat(app): Git 面板与文件树状态角标——latermd-git 接入 UI(平铺改动列表/只读 diff/确认式回滚/历史折叠区) |
| 7 | p2-creds | 凭据管理三平台封装（Credential Manager / Keychain / libsecret） | ✅完成 | feat(creds): 新建 latermd-creds——keyring 4.2 三平台凭据存取,AI key 系统凭据优先于环境变量,错误文案不含凭据值(`adbf43c`);feat(app): 设置面板 AI Provider 区——API key 走 latermd-creds 存取,密码浮窗 + 三态状态行 + AI 命令 key 闸门(`41dc89c`);经 PR #20 携带合入 main(2026-09-26 补记) |
| 11 | multi-tabs | 多标签页：同时打开多个文档（用户 2026-09-25 追加；**插队到 #8 之前执行**） | ✅完成 | feat(app): 多标签归约迁移——AI 流绑定发起标签、打开路径去重、标签命令进菜单(`3753939`);feat(app): 标签条水平滚动防溢出——ScrollArea 单行替代换行,补单行不换行回归测试(`d1ff599`,含 d1ff599 携带合入的 latermd-search 初版);口径见 decisions-pending #23 |
| 8 | p2b-theme | 皮肤批次 B：三态切换（亮/暗/跟随系统）+ 自定义皮肤文件（RON 导出至 themes/）+ 视觉打磨 | ✅完成 | feat(app): 皮肤批次 B——三态主题(跟随系统 1s 节流轮询)+themes/*.ron 皮肤文件+标准/紧凑密度;feat 与 docs 两 commits 经 PR 合入 |
| 12 | mcp-server | 本地 MCP server：五只读工具 + stdio/HTTP 双通道 + 设置页(用户 2026-09-26 追加) | ✅完成 | feat(mcp): latermd-mcp + latermd-search 下沉,PR #23 合入;docs: mcp-plan 阶段 ①-④ 全完成;口径见 decisions-pending #22 |
| 9 | p3-live-preview | Live Preview：块级 caret 路由 + 聚焦块裸源码；**共用同一 rope buffer 与 undo 栈**（roadmap 铁律） | ✅完成(v1) | feat(md): blocks() 块划分(区间连续覆盖全文);feat(app): live.rs 活动块编辑代理 + RenderMode 分派 + Cmd+/ 命令;选区扩展与内联半隐藏留 v2 |
| 10 | p3-nav | 大纲预览跳转（复用 section_to_token 映射）+ `[[wikilink]]` 双向链接（LinkHandler） | ✅完成 | `[[wikilink]]` 已落地；大纲预览跳转已落地（vendored ①类 `section_anchors()` + app 侧滚动消费） |
| 13a | shell-m1 | 外壳重构 M1：自绘无边框标题栏（六按钮/拖窗/边缘 resize）+ 三栏重排（nav/preview/central）+ layout.json 持久化 | ✅完成 | `ui/titlebar.rs`（36px 六按钮 + StartDrag + 双击最大化 + 八向 BeginResize 命中区）；`Panel::left("nav")` / `Panel::right("preview")` / 编辑器进 `Panel::left("editor")`，左右均 `show_collapsible`；新 `layout.rs` 的 `LayoutSettings` 落 `layout.json`（写盘点统一收在 `State::end_of_logic` 比对写，覆盖不产消息的面板把手路径）；左栏下限 180 (`SIDEBAR_MIN_W`)。466 测试全绿，六项门禁通过 |
| 13b | shell-m2 | 外壳重构 M2：左栏三段式（顶动作/视图导航/ScrollArea 中段/底设置行），依赖 13a | ✅完成 | `ui/sidebar.rs` 重写为四段：`top_actions`（5 个文件动作图标，`horizontal_wrapped` 窄栏换行）/ `view_nav`（四行整行选中：hover 底 + 左侧 2px accent 竖条）/ 中段 `ScrollArea` 包现有四页（`max_height` 预扣底段**含一个 item_spacing**，只扣 `NAV_BOTTOM_H` 会溢出 3px 把设置行挤出可视区）/ `settings_row`（左键 SettingsOpened(Appearance)、右键四项直达）。`ui()` 返回 `SidebarBands` 供无头测试量取四段矩形。两个坑：① 设置行原为 `horizontal` 容器，其 response 只有不可交互的 label、**收不到点击**，改为与导航行共用手绘 `icon_label_row`（`Sense::click` 打在 allocate 上，整行可点）；② egui 0.36 没有 `ui.close_menu()`，popup 默认 `CloseOnClick` 自己会关。新增 token `NAV_ROW_H/NAV_BOTTOM_H/NAV_BAR_W`。238 测试全绿，六项门禁通过 |
| 13c | shell-m3 | 外壳重构 M3：Markdown 格式工具条（compose.rs 纯函数 12 组语义 + format_bar.rs + 12 Command + selection 回填链路） | ✅完成 | 新增 `compose.rs`(16 动作纯函数 `apply(action,text,sel)→(新文本,新选区)`,11 单测覆盖包裹/去包裹/前缀替换/任务三态/块插入空行规则/字符偏移)与 `ui/format_bar.rs`(四组 `horizontal_wrapped`,B·I·S·H1-H3 走 RichText,其余自绘线段,新增 `Paragraph` 图标替代原先借用的 `Table`)。选区双向链路:`ui::editor` 每帧把 `CCursorRange` 抄进 `TabState::selection` → 归约侧 `apply_format` 写回文本并挂 `pending_selection` → 下一帧 `write_selection` 写回 TextEdit 持久 cursor + 还焦。新增 15 个 `Command` + 键位(`format_link` 取 `Cmd/Ctrl+Shift+K`,裸 `Cmd/Ctrl+K` 已被既有单测当任意空闲键占用);`EditorBuffer::replace_range` 字符区间定点编辑。三个坑:① `painter.text` 的 `impl ToString` 把 RichText 降级成纯字符串,B/I/S 的形态(加粗/斜体/删除线)全丢 —— 必须走 `WidgetText::into_galley` 再 `painter.galley`;② `ui::editor::ui` 顶到 9 个实参触发 clippy `too_many_arguments`,把 `cursor`/`selection`/`pending` 三条「UI 每帧回填」通道收成 `CursorChannel` 结构体(它们从来都是一起传的);③ `FormatAction::group()` 与 `FormatGroup::actions()` 是同一关系的两张反向表,留一张即可,删 `group`。**留口**:`EditorBuffer::replace_range` 加了但没在用(`compose::apply` 产出整篇,`replace_all` 更直接),M5 收口时决定 EditorBuffer 是否只留一条 write 路径。395 测试全绿,六项门禁通过,真机截图确认工具条四组 16 枚渲染正确 |
| 13d | shell-m4 | 外壳重构 M4：禅定模式（pre_zen 快照/限宽 720 居中/三退出入口），依赖 13a | ✅完成 | `layout.rs` 加 `PreZen=(bool,bool)` 快照与 `enter_zen`/`exit_zen`（退出**逐项还原**而非一律全开；重复进入是 no-op，第二份快照不能吞掉第一份；无快照时退化为三栏全开而非困死）。`ui::layout::draw` 开头整体分叉到新的 `draw_zen`（另一套 panel 组合，不是给三栏各加 if）。四条出口：F11 / 标题栏 Zen 键（M1 的禁用占位已实装，禅定中用 accent 着色）/ Esc（`consume_key` 在 panel 之前消费）/ 右上角 `zen_exit_button`。三处刻意偏离规格，理由写进注释：① **只对布局分叉、不对窗口 chrome 分叉** —— 规格的「全部退场」是按原生装饰写的，本产品自绘标题栏（D1）无边框下 OS 不给 chrome，照字面藏会让退出入口一起消失；② **快照是二元组而非规格的三元组** —— `editor_hidden` 在禅定下恒 true，写进存档等于每次读它都骗一次；③ `zen`/`pre_zen` 都 `#[serde(skip)]`，且 `save_to` 先按快照还原 `left/right` 再写 —— 禅定期间任何一次保存（例如切左栏视图）照直写会把「三栏全关」钉进磁盘，下次启动面对近乎空的窗口。新增 token `ZEN_TEXT_W`(720)/`ZEN_EXIT_MARGIN`/`ZEN_GUTTER`(24，**必须整数**：`Frame::inner_margin` 最终落成 i8 的 `Margin`，f32 走 `From<f32>` 会被 round 吞掉小数)。三个坑：`vertical_centered` 与 `set_max_width` 必须成对（前者居中、后者夹宽，缺一不成）；限宽取 `ZEN_TEXT_W.min(available_width())`，否则窄窗溢出成横向滚动；`UiStack::iter` 只向上走、拿不到兄弟 panel，panel 取证改用 `output.shapes` 里诈出来的文本做「三栏帧有 / 禅定帧无」对照。516 测试全绿，六项门禁通过。X11 截图取证本轮未成（xdotool 合成输入不可信的老问题复发 + 反复截到同一张旧内容），改由 shapes 层面测试兜住，禅定的像素验收随 M5 一并处理 |
| 13e | shell-m5 | 外壳重构 M5：收口（六项门禁+明暗像素采样验收+文档回写：ui-polish TOOLBAR_H 订正/adr-005 §3.2/acceptance-checklist 无边框三项），依赖 13a-d | ✅完成 | fix(app): 外壳收口三连——四栏黑条/状态栏横跨/设置挪标题栏齿轮(M5);fix(app): 编辑器文件工具栏退役、动作收口左栏图标版;修任务列表 CJK 崩溃(状态栏过期字节);docs(m5): 外壳明暗两套像素采样验收——11 项断言全过,证据入 m5-acceptance.md;状态栏右端内容挂账 #33;docs(m5): 外壳收口文档回写——ui-polish token 订正补登/adr-005 §3.2 panel 分工/验收清单无边框三项/roadmap 外壳 [x] |
| 14 | lp-v2 | Live Preview v2：内联标记半隐藏（块内分段，`**` 等标记聚焦时半透明而非整块裸源码）+ 选区扩展，依赖 #9(v1 已完成) | ⏳待开始 | roadmap「P3 深水区」剩余增强；铁律：共用同一 rope buffer 与 undo 栈 |
| 15 | backlinks | 反向链接面板：全仓扫描 `[[wikilink]]` 引用，侧栏面板列出「谁链接了当前文档」+ 点击跳转，依赖 #10(wikilink 已完成) | ⏳待开始 | roadmap「P3 深水区」剩余增强；扫描复用 latermd-search 遍历骨架 |
| 16 | icon-embed | 应用图标接入：main.rs `ViewportBuilder::with_icon(IconData)`(include_bytes assets/logo/deliverables/png/icon-64.png + image crate 解码) + latermd-app Cargo.toml 加 `image = { version = "0.25", default-features = false, features = ["png"] }`(与 vendored 层同源) + ADR-004 登记;失败回落默认图标不拦启动。图标素材已打磨就绪(LOGO-SPEC v1.1) | ⏳待开始 | 坤哥 2026-09-27 反馈「运行时图标不像」;M2 在途故未抢写 src/,由本条目落地 |
| 17 | find-replace | 单文档查找替换:Ctrl+F 浮条(当前文档内查找,Enter/N 高亮下一个,计数)、Ctrl+H 替换(单个/全部,全部替换走单条 undo,字符偏移用 ByteIndex 换算防 CJK 错位) | ⏳待开始 | 编辑器标配缺口(2026-09-27 盘点,全仓搜索有但文档内查找替换无);与多标签联动:作用于 active tab |
| 18 | autosave | 自动保存与崩溃恢复:编辑停顿 30s 或切标签时把脏缓冲落 `<doc>.latermd-draft`(与原文件同目录或状态目录);启动检测孤儿 draft 弹恢复条(恢复/丢弃);正常保存/关闭即清 draft | ⏳待开始 | 防丢是编辑器基本盘;draft 不进 Git 忽略清单之外的地方,文件树过滤 `*.latermd-draft` |
| 19 | ime-follow | Linux IME 候选框跟随:光标移动时上报 IME 位置(egui 0.36 ViewportCommand::IMEPosition 或 input IME 事件链),fcitx5 实测候选框贴光标;m0-report 验证 1 销账 | ⏳待开始 | m0 挂账「输入可用但候选框不跟随」;Win/mac 行为留真机人工项 |
| 20 | ai-adapters | AI provider 补全:Anthropic(messages API/SSE)与 Ollama(本地 /api/chat NDJSON)两个 adapter,与 openai.rs 同 trait;设置页 provider 三选一;mock 不动 | ⏳待开始 | #3 只交付了 OpenAI 兼容端;key 由用户填(decisions-pending #3),无 key 走 mock |
| 21 | image-paste | 图片粘贴/拖拽插入:编辑器 Ctrl+V 图片字节或拖入图片文件→存 `<doc名>.assets/`→光标处插相对路径引用;预览经 vendored 图片 widget 渲染;文件树过滤 assets 目录本身 | ⏳待开始(**并入 #26 的 D 段**) | 写作刚需(截图入文);PNG/JPEG/WebP 白名单,超 5MB 提示。**2026-09-27 坤哥追加「图片框 + 图床」需求,本条目内容降为 #26 的最后一段,不再单独排队** |
| 22 | cask-bump | Homebrew cask 自动回填:Release 发布 workflow 追加一步,gh api 更新 crazykun/homebrew-ailater 的 Cask latermd.rb(version + universal2 dmg sha256),失败仅告警不阻塞发布 | ⏳待开始 | macos-dmg.yml 注释明说「auto-bump 只管 Formula,cask 靠手动」——发版链路最后一块手动环节 |
| 23 | font-prefs | 编辑器字号/行距用户设置:外观页两滑杆(字号 12-24 默认 15,行距 1.2-2.0 默认 1.5),持久化 settings.json,作用于编辑器与预览正文(标题按比例) | ⏳待开始 | 密度档位特意不动字号,用户手动可调是缺口;CJK 可读性下限 12 |
| 24 | command-palette | Ctrl+P 快速打开:居中浮层,模糊搜文件树全部 md(打开)+ Command 全集(执行),↑↓ 选择 Enter 确认,Esc 关 | ⏳待开始 | 现代编辑器标配体验;复用 Command enum 与文件树快照,无新依赖 |
| 25 | export-pdf | 导出 PDF:headless 渲染(铁律 2 的验证场——latermd-render 不依赖 egui),printpdf 或 HTML→PDF 选型走 ADR;CJK 字体嵌入 | ⏳待开始 | 最重的候选,列队尾;P0 只交付了导出 HTML |
| 26 | image-bed | **图片框 + 图床**(坤哥 2026-09-27 指令,规格见 [image-plan.md](image-plan.md)):**A 段** compose 加 `FormatAction::Image` + `insert_image` 纯函数 + `Icon::Image` 自绘 + `Command::ImageInsert`(Cmd/Ctrl+Shift+I)+ 新 `ui/image_dialog.rs`(alt/URL 双输入);**B 段** 本地文件复制进 `<doc名>.assets/` + 相对路径 + 文件树过滤 + 预览相对路径解析;**C 段** 新 crate `latermd-bed`(**不 import egui**,`BedProfile` serde 落 beds.json,token 走 latermd-creds 绝不明文,HTTP 复用已有 ureq 开 multipart **不引 reqwest**,返回 URL 用手写点分路径抽取 **不引 jsonpath**)+ 设置新增第五页 `SettingsTab::Image` + 后台线程上传(序号防旧结果覆盖);**D 段** 粘贴/拖拽(吸收 #21) | ⏳待开始(**已放行 2026-09-27**,A 段并行线在途) | 四段共 4d,A 段可独立先交付。**三个坑先登记**:① vendored `label.rs:844` 的 `egui::Image::new(url)` **不解析相对路径** → app 侧喂前拼绝对 URI,不碰 vendor(vendor/README.md 登记为待上游化);② 插入打碎 TextEdit 内建 undo(compose §9 R3 同款,已知接受);③ 上传失败**只弹 notice**,绝不动用户已有内容。不做:各家对象存储 SDK / 图片编辑 / 默认自动上传 |
| 27 | ui-modern | **UI 现代化 U0–U3**(外部 P0–P4 建议查证后重排,见 [ui-modernization.md](ui-modernization.md)):**U0** 扩 `tokens.rs`(抄 armas 数值:圆角 6 / 输入高 36 / padding 12·8 / 字号 14)+ 抄 egui-thematic 九套色板转 `themes/*.ron`;**U1** Inter(Regular/Medium/SemiBold)+ 已有 CJK fallback 混排;**U2** 即 #26;**U3** 自研 20 行动效(替代 GPL 的 egui_transition_animation) | ⏳待开始(**已放行 2026-09-27**,范围 U0/U1/U3;U4 phosphor 仍待拍板 #35) | U0–U3 合计 ~5.2d 且 **零新增第三方依赖**。**已否决四条**:egui-thematic(27 下载/锁 egui 0.33/与自有主题重复)、Armas(23 下载月/锁 egui 0.33/推翻 M1-M3 自绘控件,工作量是数周不是「中」)、backdrop-blur-egui(grab-pass 需 **glow** 与 AGENTS §5 冲突、own-loop 要求不用 eframe)、egui_transition_animation(**GPL-3.0-or-later**,LaterMD 是 MIT 不合规)。**待拍板一条**:egui-phosphor 整体迁移与否,见 decisions-pending #35 |
| 28 | emoji-panel | **Emoji 面板**(坤哥 2026-09-27 指令,规格见 [emoji-plan.md](emoji-plan.md)):**E1** `Icon::Emoji` 自绘笑脸 + 工具条末尾挂按钮 + 新 `ui/emoji_panel.rs`(`egui::Window` 形态,与 image_dialog 一致)+ 8 分类网格(每类 ≤40 枚)+ 点选插入走 `compose::insert_emoji`(char 索引,emoji 在 Rust 里是单 char 故天然安全);**E2** 搜索(中文名/英文名/短码三路匹配)+ 分类切换 + 最近使用落 settings.json;**E3** `Fonts::has_glyph` 建表时过滤缺字形 + 三平台真机目视;**E4**(可选) `:gemoji:` 短码自动补全 | ⏳待开始(**已放行 2026-09-27**,数据源按 #36 默认甲) | 合计 ~1.3d,E1 仅 0.5d 可独立先交付,与 #26 无依赖可并行。**三条实测硬事实**(均核到 epaint-0.36.2 源码):① `FontDefinitions::default()` **已注册 `NotoEmoji-Regular` + `emoji-icon-font`** 且 emoji 为第一优先级(fonts.rs:508-545)→ **零新增依赖即可渲染**,`fonts.rs::install()` 无需改动;② **彩色 emoji 上游不支持**(字形一律白色入字体图集,image.rs:350-373)→ 应用内黑白、导出 HTML 仍是彩色,须在面板写明以免被当 bug;③ 网传的 `monochrome_emoji_fonts` feature **在 0.36.2 已移除**(并入 default_fonts),照抄编不过。**不进 `FormatAction`**(其语义是「可由 text+sel 推导的纯文本变换」,emoji 字符必须由面板提供),与 Image 同属「对话框类动作」,在工具条末尾单独渲染。**七坑先登记**:Noto Emoji 覆盖落后于 Unicode 17(用 has_glyph 过滤豆腐块)/ ZWJ 序列多码位(Backspace 只删末组件,已知接受)/ 肤色变体不收 / undo 被打碎(compose §9 R3 同款)/ 翻页致字体图集膨胀 / 单测不断言渲染结果(依赖真实字体会 flaky)/ 黑白非彩色。**待拍板**:数据源「内置精简表 ~300 枚(推荐,零依赖)」vs「引 `emojis` crate(1MB/Unicode v17)」,见 decisions-pending #36 |

状态图例：⏳待开始 → 🔄进行中 → ✅完成 / ❌挂起（3 次失败）/ ⛔受阻（依赖挂起）。

> **#13 已放行**（2026-09-26 坤哥指令「看还有什么未完成的，加入流水线」）：D1–D5 按规格默认值
> 拍板入档（decisions-pending #30，`LATERMD_NATIVE_DECORATIONS=1` 逃生口保留），拆 13a–13e 五棒顺序推进；
> 无边框三平台 resize 风险由 M5 像素验收 + acceptance-checklist 无边框三项兜底，Win/mac 真机仍走人工清单。

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

**#11 multi-tabs**（用户 2026-09-25 追加，插队于 #8 前）：
- tabs-core：`TabState { id: u64, doc: DocumentState, scroll: … }`、`Vec<TabState> + active_tab`；`Message::TabOpen/TabClose/TabActivate/TabCloseActive`；关闭脏标签确认模态；Ctrl+Tab 切换、Ctrl+W 关闭；编辑器 buffer/预览/大纲/搜索跳转/选中行按 active_tab 隔离。
- tabs-ui：顶部标签条（`egui::TabBar` 或自绘；与顶部菜单栏并存），标签拖拽排序延后（notes 记录）。

**#8 p2b-theme**：Theme 加三态（亮/暗/跟随系统，`dark-light` crate，Linux 失灵回退手动）；Theme serde 导出 RON 至用户配置目录 `themes/`，启动扫描可加载；视觉打磨（间距/圆角 token 统一、滚动条、hover 态、编辑器行距）。

**#9 p3-live-preview**：编辑器加 `render_mode` 标志（一个编辑器两种模式，共用 rope buffer/undo）；光标所在 block 用 source_span 显示源码，其余走富渲染；块间 caret 路由（↑/↓ 跨 block、Home/End）。参考 vendor README 的 render_token_range。

**#10 p3-nav**：大纲点击滚动预览到对应 section（section_to_token 映射 + scroll_to_rect）；`[[wikilink]]` 解析（latermd-md 层加语法或预处理）+ LinkHandler 点击打开文件树中同名 md。

**#13 shell-redesign**（坤哥 2026-09-26 指定形态；D1–D5 已按默认拍板见 decisions-pending #30）：规格全文在 `docs/ui-shell-redesign.md`，这里只列 ordered milestones（对应队列表 13a–13e 五棒，写脚本时按棒取对应 M 段规格）。
- **M1 标题栏 + 三栏重排（1.5d）**：新 `ui/titlebar.rs` —— `main.rs` 加 `with_decorations(false)`；自绘 36px 条（文档名 + dirty 星号 / 右端 关闭左 · 关闭右 · 禅定 · 最小化 · 最大化 · 关闭 六按钮）；拖窗走 `ViewportCommand::StartDrag`，双击标题区走 `Maximized(!info.maximized)`；无边框丢 resize 手柄，四边各留 6px 命中区发 `BeginResize(八方向)`。新 `layout.rs` 存 `LayoutSettings{left,right,zen,left_view}` 落 `layout.json`。`ui::layout::draw` 的 panel 顺序改为 top(titlebar) → top(menubar) → left("nav") → right("preview") → `CentralPanel`(编辑器) → bottom(statusbar)；左右两个都用 `show_collapsible` 吃 `&mut bool`。
- **M2 左栏三段式（1.5d）**：重写 `ui/sidebar.rs` —— 顶段五个文件动作图标、次段四行视图导航（整行选中：`selected_bg` 底 + 左侧 2px `accent` 竖条）、中段 `ScrollArea` 包现有四页内容（`max_height = available - NAV_BOTTOM_H` 吃掉剩余）、底段齿轮设置行。左栏下限 160→180。
- **M3 格式工具条（3d）**：新 `compose.rs`（**纯函数**，不依赖 egui）：`apply(FormatAction, text, sel: Range<char>) -> (String, Range<char>)`，12 组语义按规格 §6.2 表实现（含 toggle off、跨行前缀替换、任务三态、CJK 多字节）+ 单测。新 `ui/format_bar.rs`；新增 12 个 `Command` + 键位（见规格 §6.3）；`EditorBuffer` 加 `replace_range(char_range, text)`；`TabState` 加 `selection` / `pending_selection` 走「UI 每帧回填 → 归约 → 下帧写回」链路（copy 既有 `OutlineCursor` 手法）。
- **M4 禅定（1d）**：`layout.rs` 加 `pre_zen` 快照，进入=三个全关 + 预览交由 `CentralPanel` 限宽 720 居中；退出还原到**进入前**的组合，Esc / F11 / 右上浮出按钮三入口。
- **M5 收口（1.5d）**：六项门禁 + 明/暗两套像素采样验收 + 文档回写（ui-polish 的 TOOLBAR_H 订正、adr-005 §3.2 补新 panel 分工、acceptance-checklist 增无边框三项）。

## 人工待办（自动循环不做）

- ~~#13 的 D1–D5 决策~~（2026-09-26 已拍板：按规格默认值，见 decisions-pending #30）
- IME 真机实测（Win11 微软拼音 / macOS 简体拼音）——M0 挂账项
- Win/mac wgpu 真机启动验证（13a 无边框化后需一并复测）
- 三平台打包真机验收：`feature/p0-packaging` 分支（cargo-dist + universal2 dmg + cask 模板已就绪）等待人工验收合并，**自动循环不碰该分支**（见 decisions-pending #1）
- AI provider 真实 API key 配置
- ~~视觉终审与发布（打 tag 触发 Release）~~ v0.0.1 已发布（2026-09-26 tag + GitHub Release 产物就位）；剩余 = 发布产物真机验收（下述三项）与发布后收尾清单（acceptance-checklist）
