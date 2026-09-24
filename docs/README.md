# LaterMD 架构决策记录

本目录存放项目的技术决策记录（ADR）。每条 ADR 的结论都由**实测数据**支撑，不是推测。

## 文档索引

| 文档 | 主题 | 状态 |
|---|---|---|
| [adr-001-gui-and-architecture.md](adr-001-gui-and-architecture.md) | GUI 框架选型、workspace 分层、里程碑 | 已接受 |
| [adr-002-platform-renderer-wysiwyg.md](adr-002-platform-renderer-wysiwyg.md) | 平台基线、图形后端、WYSIWYG 形态 | 已接受 |
| [adr-003-renderer-and-ecosystem-audit.md](adr-003-renderer-and-ecosystem-audit.md) | 渲染层与 Rust 生态审计 | 已接受 |
| [adr-004-technical-stack.md](adr-004-technical-stack.md) | 技术栈与依赖版本清单、打包方案、工具链 | 已接受（2026-09-24 重构） |
| [adr-005-layout-and-sidebar.md](adr-005-layout-and-sidebar.md) | 三栏布局、侧边栏、字体与并发模型 | 已接受 |
| [roadmap.md](roadmap.md) | 阶段划分、验收标准、crate 增量时机、风险登记册、当前位置 | 已接受（**排期唯一事实来源**） |
| [vendor-upgrade-checklist.md](vendor-upgrade-checklist.md) | egui_markdown vendor 与升级操作清单 | **已执行**(2026-09-24 六项全绿) |
| [m0-report.md](m0-report.md) | M0 技术验证结论（IME / 帧率 / 三 target 实测数据） | 已产出（Linux 项完成；IME 与 Win·mac wgpu 待真机） |

## 决策总表（一句话版）

| 议题 | 结论 | 出处 |
|---|---|---|
| GUI 框架 | **egui + eframe 0.36.2** | ADR-001 §2 |
| Markdown 解析 | **pulldown-cmark**（不是 comrak） | ADR-003 §4 |
| 渲染层 | **vendored egui_markdown (membrane-io)**，升级到 egui 0.36.2 | ADR-003 §2、§6 |
| 图形后端 | **wgpu**（eframe 默认），glow 仅作 env-var 逃生口 | ADR-002 §3 |
| 平台基线 | Windows 11 / macOS 14；不支持 Win10、macOS 13 | ADR-002 §2 |
| WYSIWYG 形态 | **B：Live Preview**；True WYSIWYG 列入 non-goals | ADR-002 §4 |
| 打包编排 | **axodotdev/cargo-dist** v0.33+ | ADR-004 §4 |
| 工具链 | **rustc 1.98.0** 钉死 | ADR-004 §9 |
| 布局 | `Panel::left` × 2 + `CentralPanel`；**不用 `SidePanel`**（0.36.2 已移除） | ADR-005 §2 |
| App trait | `logic` / `ui` 二分；**无 `update`** | ADR-005 §2.3 |
| 侧边栏折叠 | `Panel::show_collapsible`（非手写 `bool`） | ADR-005 §3.3 |
| 搜索引擎 | `ignore` + `grep-searcher` + `regex`；**不用 `grep-regex`** | ADR-005 §4.2 |

## 三条贯穿全局的约束

1. **单一解析器。** 全项目只允许存在一个 Markdown 解析器（pulldown-cmark），避免预览与导出出现两套方言。
2. **`core` 不依赖 UI 框架。** 渲染层只产出绘制指令，不 import `egui`，以便将来做 headless CLI 导出器。
3. **AST 是 AI 增强的架构基础。** 所有 AI 修改在 token/AST 层操作，而非操作渲染后的盒子。

## 修订记录

| 日期 | 变更 |
|---|---|
| 2026-09-24 | 初版：ADR-001/002 建立，GUI 与平台决策 |
| 2026-09-24 | ADR-003：生态审计。推翻「egui_markdown 已死」的初判（实际是仓库迁至 membrane-io 并持续活跃）；推翻「单 Galley 无法虚拟化」（实际已实现视口剔除） |
| 2026-09-24 | ADR-004 + roadmap：修正打包工具（cargo-dist 未归档的是 astral 分支，官方为 axodotdev）；大幅削减功能范围 |
| 2026-09-24 | vendor-upgrade-checklist：0.34→0.36.2 升级实测（24 个错误），并发现其中 9 个是上游 HEAD 自身 bug（在其自称支持的 egui 0.34.3 上也编译失败） |
| 2026-09-24 | ADR-005 + roadmap：三栏布局。修正 `SidePanel` / `App::update` 两个已消失的 API；大纲从 P3 前移到 P0（廉价版 3 天）；`Token::source_span` 提前到 vendor 阶段一次性完成 |
| 2026-09-24 | **文档体系一致性修订**：① ADR-004 重构为纯技术栈 ADR（原「最终汇总」中的功能范围/排期是过期快照，与 roadmap 冲突，已移交）；② 确立 roadmap.md 为排期唯一事实来源，ADR-001 §5 加修订注；③ P3「全文检索」与 P1 侧边栏搜索重复，删除；④ roadmap 阶段 0 验收补齐第六项 clippy；⑤ 新增 crate 增量创建表、风险登记册、阶段出口条件、M0 产出物约定（m0-report.md）；⑥ 修复 checklist 笔误 |
| 2026-09-24 | **分发策略修订**：确认无 Apple Developer 账号与签名证书。分发改为 GitHub Release + Homebrew tap `crazykun/homebrew-ailater`（cask 复刻 lscreen 模式：universal2 dmg + postflight 去 quarantine + livecheck）。签名公证相关结论在 ADR-004 / roadmap / AGENTS.md 中同步作废改写 |
| 2026-09-24 | **阶段 0（Vendor 适配）完成**：subtree 引入 egui_markdown@4f3075f，egui 0.34→0.36.2（15 错误 + 未预见的 TexturesDelta drop 检查 10 测试），删除 membrane feature（23 处），`Markdown::spans` 平行数组落地（大纲/Live Preview prerequisite）。check.sh 六项全绿、100 测试通过、latermd-app 空窗口在 Linux/X11 实跑通过 |
| 2026-09-24 | **M0 中间态**：`docs/m0-report.md` 产出（Linux 侧三项通过 —— wgpu/Vulkan 软件 adapter 起动、流式追加 ~9 ms、中文渲染无方块且字体方案定案为 cfg 原生候选表）；bench 全量重跑数据回填。CI 重写为 gate（六项门禁 + glow 编译）+ 三平台 build 矩阵，工具链钉 1.98.0。剩余真机项（IME、Win11 DX12 / macOS Metal）未测，M0 结论未最终放行 |
| 2026-09-24 | **roadmap：新增「界面美化与皮肤系统」专题**。批次 A（Light/Dark 双皮肤 + 外壳/正文/代码高亮三层联动 + 持久化）并入 P0 主题条目并扩至 1.5 周；批次 B（三态切换 + 自定义皮肤文件 + 视觉打磨）设为阶段 4.5（P2.5，P3 前视觉定稿）；批次 C（皮肤市场等）明确不做。实测盘点：vendored `MarkdownStyle` 原生成对明暗字段、`code_theme` 不传自动跟随明暗、缓存 hash 含 `dark_mode`，皮肤系统全程不动 vendor。风险登记册新增 #8（Linux 跟随系统主题无统一规范） |
| 2026-09-25 | **P0 模块批量实现 + M0 报告产出**：latermd-md（解析与大纲数据层）、三栏布局骨架、源码编辑与实时预览、文件新建/打开/保存/另存、大纲面板与光标跳转、HTML 导出（latermd-export）、主题切换与持久化、统一快捷键与菜单栏、文件树基础版，另含大纲跳转 span 钳制与保存/导出原子落盘修复（共 11 commits）。M0 报告补齐中文字体注入落地、全量 bench 数据与六项验证状态；roadmap「当前位置」更新为 M0 真机项挂账、P0 进行中 |
