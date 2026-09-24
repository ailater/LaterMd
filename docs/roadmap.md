# LaterMD 演进路线图

日期: 2026-09-24
状态: 已接受
关联: 全部 ADR

> 本文件是 **动态清单**，不重复各 ADR 的论证，只记录阶段划分、验收标准与当前位置。
> 论证见 [adr-001](adr-001-gui-and-architecture.md)、[adr-002](adr-002-platform-renderer-wysiwyg.md)、[adr-003](adr-003-renderer-and-ecosystem-audit.md)、[adr-004](adr-004-technical-stack.md)、[adr-005](adr-005-layout-and-sidebar.md)。

---

## 当前位置

```
[x] Vendor 适配   2026-09-24 完成(check.sh 六项全绿,spans 已加入)
[ ] M0 技术验证   ← 当前
[ ] P0 骨架
[ ] P1 差异化
[ ] P2 版本层
[ ] P3 深水区
```

> Vendor 适配实测纪要:15 个升级错误全部修复;另有 checklist 未预见的 **TexturesDelta drop 检查**(egui 0.36 新增)导致 10 个测试失败,已在测试中补 `output.textures_delta.clear()`。`source_span` 以**平行数组**形态落地(`Markdown { s, tokens, spans }`),不动 15 个 enum 变体,不变量 `spans.len() == tokens.len()` 有单测。差异全记录在 [vendor/egui_markdown/README.md](../vendor/egui_markdown/README.md)。100 个测试全绿;`latermd-app` 空窗口在 Linux/X11 实际运行通过(wgpu adapter 正常)。

---

## 阶段 0：Vendor 适配（3-5 工作日）

**目标**：消除全部升级不确定性。

**动作**：

1. `git subtree add` vendor `membrane-io/egui_markdown`（含 `egui_markdown_style` 子 crate、`tests/`、`check.sh`）
2. 修 15 个真实的升级错误（另有 9 个是上游自身 bug，直接删）—— 清单见 [vendor-upgrade-checklist.md](vendor-upgrade-checklist.md)
3. **同步加入 `Token::source_span`**（这是唯一会碰 `Token` 定义的机会，同时服务大纲与 Live Preview）
4. 钉 `rust-toolchain.toml` 到 1.98.0

**验收**（与上游 `check.sh` 六项完全一致，一项不能少）：

```bash
cd vendor/egui_markdown
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --no-default-features -- -D warnings
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo doc --no-deps --all-features
```

全绿。（前提：已删除 `membrane` feature，否则第三轮 clippy 必失败 —— 见 vendor-upgrade-checklist §6。）

---

## 阶段 1：M0 技术验证（2 周）

**目标**：三条不过则不继续。这是**唯一允许失败便宜的阶段**。

| # | 验证项 | 验收标准 | 关联 ADR |
|---|---|---|---|
| 1 | **IME 中文输入** | Win11 微软拼音 + macOS 14 简体拼音：候选框跟随 caret、不吞字、不抢焦点 | ADR-002 §6 |
| 2 | **长文档性能** | 10 万字 md 滚动到中部 ≥ 55fps（验证视口剔除生效） | ADR-002 §3.5 |
| 3 | **wgpu 三 target** | Win11(DX12) / macOS14(Metal) / Linux(Vulkan) 均能启动且报告合理 adapter | ADR-002 §6 |

**补充建议（来自 P0 风险前移）**：

| # | 附加验证 | 目的 |
|---|---|---|
| 4 | **流式性能边界** | mock LLM 每 100ms 喂一个 chunk，跑上游现成的 `bench_render_scroll_code_streaming`，记录 500 / 2000 / 10000 行帧率 |
| 5 | **中文渲染** | 在 Deepin 上确认中文不显示方块；同时定案字体方案（`fontdb` / `font-kit` / cfg 原生） |
| 6 | **tokio ↔ egui 通道** | `mpsc` + `request_repaint()` 往返无锁竞争 |

第 4-6 项很便宜，但第 4 项是唯一有真实信息量的。

**载体约束**：M0 用 vendored crate 自带的 example / bench / 最小 `latermd-app` 窗口骨架做验证，**不写产品代码**。验证代码可以丢，结论不能丢。

**产出物（硬性）**：无论通过与否，M0 结束时必须新增 [docs/m0-report.md](m0-report.md)：三条主验证 + 三条附加验证的实测数据（帧率数字、adapter 信息、IME 行为记录）、结论（继续 / 换 iced 备选 / 停止）。不过则不进入 P0，没有例外。

---

## 阶段 2：P0 骨架（8-10 周）

**目标**：能写下一篇技术文档并交付给别人看。

### 范围

| 模块 | 内容 | 周期 |
|---|---|---|
| 三栏布局 | `Panel::left` × 2 + `CentralPanel`，全部可调宽；侧边栏 `show_collapsible` | 0.5 周 |
| 编辑器 | 双栏源码编辑 + 实时预览 | 2 周 |
| 文件操作 | 新建 / 打开 / 保存 / 另存为 | 0.5 周 |
| Markdown | CommonMark + GFM（表格、任务列表、删除线、脚注） | 已由 vendor 提供 |
| 代码高亮 | syntect + 复制按钮 | 已由 vendor 提供 |
| 导出 | HTML | 1 周 |
| 主题 | Light / Dark / 自定义（`MarkdownStyle` + serde） | 1 周 |
| 快捷键 | `Ctrl` / `Cmd` 自动适配 | 0.5 周 |
| **文件树**（基础版） | `ignore` + 懒加载 + `.gitignore` + 点击打开 + 当前文件高亮 | 1.5 周 |
| **大纲**（廉价版） | AST 提取标题 + 点击跳编辑器光标。**不跳预览** | 0.5 周 |
| 打包 | 三平台产物发 GitHub Release；macOS universal2 dmg 经 [crazykun/homebrew-ailater](https://github.com/crazykun/homebrew-ailater) cask 分发（复刻 lscreen 模式） | 1 周 |

> **分发依赖（非阻塞）**：无签名证书路线已定（AGENTS.md §5）。macOS cask 复刻 `lscreen` 写法（universal2 单 dmg + postflight 去 quarantine + livecheck）；接入 tap 的 auto-bump 流水线只需在其 `FORMULAS` 表 / `Casks/` 加一项，随时可办，无审批等待。

### crate 增量创建表（防止「开工即 8 个空 crate」）

ADR-001 §3 的 8-crate 结构是**终态**，不是开工指令。空 crate 骨架是过早抽象（违反 AGENTS.md §8）。按下表增量创建：

| 时机 | 新建 crate | 理由 |
|---|---|---|
| 阶段 0（Vendor） | workspace 根 + `latermd-app`（空窗口） | M0 验证需要载体 |
| P0 开工 | `latermd-md`、`latermd-editor`、`latermd-export` | 解析/token 层与 rope/光标层是三条铁律的落点，必须有独立边界；HTML 导出是 P0 验收项 |
| P1 开工 | `latermd-ai` | provider trait 与流式 |
| P2 开工 | `latermd-git` | 只读 Git |
| 出现第二个消费者时 | `latermd-render`、`latermd-core` | render（绘制指令 IR）在只有 egui 一个后端时没有存在价值；core（状态机/DTO）的职责在 P0 由 `latermd-app` 承担，等 AI 消息总线复杂化后再抽 |

### 验收

- [ ] 三平台可安装
- [ ] 能连续写 1 小时技术文档不崩、不卡
- [ ] 导出的 HTML 可直接交付他人阅读
- [ ] `.md` 文件保持原样（无格式化篡改）

---

## 阶段 3：P1 差异化（+7.5-9.5 周）

**目标**：AI 成为产品的差异点，而非附加功能。

| 模块 | 内容 | 周期 |
|---|---|---|
| AI 流式写作 | `heal()` + `LinkHandler` + provider trait（OpenAI / Anthropic / Ollama） | 3 周 |
| `ai://` 链接协议 | `.link_style()` + `.click()` 拦截 | 0.5 周 |
| AI 指令块 | `.is_block_widget()` → `.block_widget()` | 1 周 |
| AI commit message | — | 0.5 周 |
| AI 摘要 / 大纲 | — | 1 周 |
| **搜索**（即全文检索，P3 不再重复） | `ignore` + `grep-searcher` + `regex`，300ms 防抖 + 可取消 + 流式结果 | 1.5 周 |

### 关键约束

- **AI 流式预览必须用稳定 widget id**，不得含 `content.len()`（否则每个 token 清空临时缓存，禁用追加增量高亮）
- 所有 AI 修改在 token/AST 层操作，不操作渲染后的盒子
- `Mut` 操作通过 channel 回传，不在后台线程直接改 UI 状态

---

## 阶段 4：P2 版本层（+4.5-6.5 周）

**目标**：Git 成为可信的时间轴，**只做只读**。

| 模块 | 内容 |
|---|---|
| Git 只读集成 | 状态、历史、diff、blame、回滚 |
| 文件树 Git 标记 | M / A / U / ? 与 `git2::statuses()` 联动 |
| 凭据管理 | 三平台封装（Credential Manager / Keychain / libsecret） |

**明确不做**：rebase、cherry-pick、LFS、submodule、多仓库。

---

## 阶段 5：P3 深水区（+8-10 周）

| 模块 | 内容 | 依赖 |
|---|---|---|
| **Live Preview** | 光标所在 block 显示源码，其余富渲染 | Token source_span（阶段 0 已做） |
| 大纲预览跳转 | 复用现有 `section_to_token` 映射 | — |
| 双向链接 `[[wikilink]]` | 通过 `LinkHandler` 实现 | — |

> 全文检索已并入 P1 侧边栏搜索，本阶段无检索条目。

### Live Preview 三块补工

1. `Token::source_span` —— **阶段 0 已完成**
2. 块级 caret 路由（↑/↓ 跨 block、Home/End、选区扩展）—— 3-4 周，无捷径
3. 内联标记半隐藏 —— v1 可简化为「聚焦时整条源码裸出来」

### 一条不能违反的原则

> **源码模式与 Live Preview 必须共享同一个 rope buffer 和同一批渲染组件**，区别仅在于是否应用那条切换规则。
>
> 否则 Cmd+/ 切换会丢光标、丢 undo 栈。

---

## 非目标（永久）

写进 README，避免范围蔓延：

- True WYSIWYG（Word / Notion 形态）
- 知识图谱、语义搜索、MOC
- 多模态嵌入
- rebase / cherry-pick / LFS / submodule / 多仓库
- 小说助手、闪卡、日记洞察

> 判断标准：如果一项功能不能让「写下一篇技术文档」变得更快，它就不在 P0-P2。

---

## 周期汇总

| 阶段 | 周期 | 累计 |
|---|---|---|
| Vendor 适配 | 3-5 工作日 | ~1 周 |
| M0 验证 | 2 周 | ~3 周 |
| P0 骨架 | 8-10 周 | ~13 周 |
| P1 差异化 | +7.5-9.5 周 | ~22 周 |
| P2 版本层 | +4.5-6.5 周 | ~28 周 |
| P3 深水区 | +8-10 周 | ~38 周 |

约 **9 个月**（单人，全职）。仅供规划参考 —— 实际会随 M0 结论调整。

---

## 风险登记册

| # | 风险 | 等级 | 缓解 | 触发信号 | 应对 |
|---|---|---|---|---|---|
| 1 | **IME 缺陷**（吞字 / 候选框不跟随 / 抢焦点） | 高 | M0 第 1 条验证 | M0 实测不过 | 停止或换 iced 备选（ADR-001 §2.5），不硬修 egui |
| 2 | **单人 9 个月工期** | 高 | 每阶段出口评审；P0 完成即有自用价值 | 连续两阶段超期 50% | 砍 P2/P3 范围，P1 收敛为「流式写作」单功能 |
| 3 | **无签名的信任门槛**（Gatekeeper「已损坏」/ SmartScreen 警告） | 低 | brew cask `postflight` 去 quarantine；README 写明 Windows「仍要运行」指引 | 非 brew 用户首次打开受阻 | 已是 lscreen 验证过的成熟路径，不新增投入 |
| 4 | **上游 egui_markdown 停更** | 中 | 已 vendor（subtree），不受上游发布节奏约束 | 上游 6 个月无提交 | fork 并公开维护；升级决策权完全在我们 |
| 5 | **CharIndex 强类型重构引入 off-by-one** | 中 | vendor 升级后 `cargo test` 必跑（checklist §8 第 4 步） | tests/cache、indent、truncate 回归 | 以测试为准逐个修，不带病合并 |
| 6 | **egui 上游 churn**（季度级破坏性发版） | 中 | 钉 0.36.2；升级是显式 ADR 决策，不追新 | 安全公告 / 必需 bugfix | 单独开升级 ADR，走 vendor-upgrade-checklist 流程 |
| 7 | **cask 停更**（lscreen 曾因 dmg 资产命名变化导致 cask 停在旧版） | 中 | `livecheck :github_latest` + tap 的 auto-bump 流水线；Release 资产命名在 cargo-dist 配置中钉死，不随版本改格式 | 用户 brew 拿不到新版本 | 命名变化时同步改 cask URL 模板（一次 5 分钟） |

---

## 阶段出口条件（维护规则）

每个阶段完成时，**必须**做三件事，否则视为未完成：

1. 更新本文件「当前位置」的 checkbox 与日期。
2. 在 docs/README.md 修订记录表追加一行。
3. 产出该阶段的验收证据（M0 = m0-report.md；Vendor = check.sh 六项全绿截图或日志；P0+ = 三平台安装包 + 验收清单勾选）。

---

## 持续项：跨平台 CI

```yaml
strategy:
  matrix:
    os: [ubuntu-latest, macos-latest, windows-latest]
steps:
  - uses: actions/checkout@v4
  - uses: dtolnay/rust-toolchain@1.98.0    # 不能用 @stable
  - run: cargo test --release
  - run: cargo build --release
  - run: cargo dist build
```

追加 `aarch64-pc-windows-msvc` 发布目标。

> 以上为**结构示意**。落地时以 `cargo dist init` 生成的 workflow 为准（它产出的 `--target` 矩阵、缓存与签名步骤是官方维护的），并追加 `Swatinem/rust-cache`。vendor 的 `check.sh` 六项门禁需接入 CI。
