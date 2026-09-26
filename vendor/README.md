# vendor/ 变更登记（LaterMD 私有）

> 本文件在 subtree 前缀 `vendor/egui_markdown/` **之外**，是 LaterMD 对上游私有改动的唯一登记处。
> LaterMD 的说明、记录**一律不写进 `vendor/egui_markdown/` 内部**——那会污染 `git subtree pull` 与发往 fork 的补丁。
> 规范正文：[AGENTS.md](../AGENTS.md) §6 第 9 条；操作细节：[docs/vendor-upgrade-checklist.md](../docs/vendor-upgrade-checklist.md) §9。

## 上游信息

- canonical 仓库：<https://github.com/membrane-io/egui_markdown>（`iamseeley/egui_markdown` 301 跳转到此；上游 Cargo.toml 的 `repository` 字段是转移前的旧地址，以跳转后的为准）
- 许可：MIT OR Apache-2.0，回馈上游无法律障碍
- 引入基点：`4f3075f`（2026-09 `git subtree add --squash`）
- 上游 main 状态：截至 2026-09-24 仍在 egui 0.34（我们的 0.36.2 升级对其是有效 PR 素材）

拉取上游更新（拉完重跑六项门禁，见 checklist §6；与本地魔改的冲突按下方登记表逐类解）：

```bash
git subtree pull --prefix=vendor/egui_markdown \
  https://github.com/membrane-io/egui_markdown.git main --squash
```

## 提交规范：三类拆分

对 `vendor/egui_markdown/` 的任何改动，commit 必须按主题拆分，**三类不得混**：

| 类别 | 范围 | 去向 |
|---|---|---|
| ① 上游可合 | 版本升级、API 迁移、token span 透传等对任何使用者都通用的改动 | 独立 commit、独立可 cherry-pick；设计时即按上游 CONTRIBUTING 标准（单主题、带测试、CHANGELOG `[Unreleased]`） |
| ② 私有删改 | `membrane` feature 及其 cfg 代码、配套测试 | 永久留在本仓 |
| ③ 仓库接驳 | 删上游 `[workspace]` 段 / 嵌套 `rust-toolchain.toml`、挂根 workspace、vendor 头注释 | 永久留在本仓；每次 subtree pull 后可能需重放 |

规则：

- 同一文件混多类改动时按 hunk 拆（`git add -p`）；同一 hunk 同时含两类时归 ①，fork 侧重写时再补齐 membrane 分支。
- commit message 用 `vendor:` 前缀；提交后在下方登记表补 hash 与类别。
- ① 类是将来 fork 上游提 PR 的搬运单元，**先登记后搬运**，合入上游后在此标记，subtree pull 冲突面随之缩小。

## 变更登记表

| 状态 | commit | 类别 | 说明 |
|---|---|---|---|
| 已提交 | b9d5070 | ① | `LinkHandler` 新增围栏代码块级 block widget 扩展点：`is_block_code_widget` / `block_code_widget`（link.rs），`needs_segmentation` / `build_layout` / `render_token_range` 三处分段同步（layout.rs / label.rs），测试 tests/block_code_widget.rs。首个消费方是 app 侧 ```ai 指令卡：按 info string 判定，与既有链接 block widget 同构，上游可合（注：随 feat(app) commit 一并入库，未拆独立 `vendor:` 前缀 commit） |
| 已提交 | 本轮 | ① | 新增 `SectionAnchor` 与 `section_anchors()`（label.rs）+ 在 `render_galley` 两条分支（可交互 / 不可交互）记录各 section 的顶部 y 到 `ui.data`，测试 tests/section_anchors.rs。动机：`MarkdownLabel` 把内容画进单个 galley，调用方（大纲面板）无从知道某一节落在哪，无法做「滚动到这一节」。纯新增能力，不改动既有渲染路径，上游可合 |
| 待提交 | — | ① | egui 0.34 → 0.36.2：两个 Cargo.toml 的版本三件套 + pulldown-cmark 0.13.4 + `layout.rs` `ByteIndex`/`ByteRangeExt` 迁移 |
| 待提交 | — | ② | 删 membrane feature：Cargo.toml feature 行、`layout.rs` cfg 块、`style.rs` `InlineCodeStyle` 五个 membrane 字段与 `Stroke` 导入、`tests/indent.rs` 整文件、`tests/width.rs` 的 membrane 测试 |
| 待提交 | — | ③ | 删上游 `[workspace]` 段（两个 Cargo.toml）与嵌套 `rust-toolchain.toml`；Cargo.toml 头部 vendor 注释 |

> 根目录 `Cargo.toml`、`rust-toolchain.toml` 是主仓新文件，不属于 vendor 改动，随 ③ 之后用主仓自己的 commit 提交。

## membrane feature 删除理由（AGENTS.md §6.8 要求的记录）

- 它是上游自家产品（membrane）的定制层，对 LaterMD 无价值。
- 0.36 升级实测带来 9 个额外编译错误（`LeadingSpace` 在 0.36 已彻底移除、`TextFormat::bg_stroke`/`bg_corner_radius` 不存在），删除后 `--all-features` 门禁才能过。
- 结论与实测数据：[docs/vendor-upgrade-checklist.md](../docs/vendor-upgrade-checklist.md) §3/§5。

## 向上游回馈（fork PR）

1. fork `membrane-io/egui_markdown` 并 clone；
2. 从主仓导出 ① 类改动，在 fork 内应用（去掉两级前缀）：
   ```bash
   git diff <①类基点commit> -- vendor/egui_markdown > /tmp/upgrade.patch
   git apply -p2 /tmp/upgrade.patch   # 在 fork clone 里执行
   ```
3. **手工补齐 membrane 分支的 0.36 迁移**（本仓已删膜，fork 里 feature 还在）：`LeadingSpace` 悬挂缩进、`bg_stroke`/`bg_corner_radius` 需 0.36 等价实现，或向作者说明取舍；
4. 过上游 `./check.sh`，按 CONTRIBUTING 更新 `CHANGELOG.md` 的 `[Unreleased]`；
5. PR 目标：canonical 仓库 `membrane-io/egui_markdown`。单人维护，响应速度预期放低；每合入一块，本地 subtree diff 小一块。
