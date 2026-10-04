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
| 待上游化 | — | ① | 给 `Token::Image` 加 `base_dir`（相对图片路径解析）：Markdown 渲染器没有「文档目录」概念，相对地址一律加载失败。LaterMD 侧暂以 app 层字符串改写绕过（喂预览前把相对图片目标拼成 `file://` 绝对 URI，`crates/latermd-app/src/ui/preview.rs` 的 `resolve_relative_images`），vendored 一行未动；若上游采纳 base_dir，该绕过可整体删除 |
| 已提交 | b9d5070 | ① | `LinkHandler` 新增围栏代码块级 block widget 扩展点：`is_block_code_widget` / `block_code_widget`（link.rs），`needs_segmentation` / `build_layout` / `render_token_range` 三处分段同步（layout.rs / label.rs），测试 tests/block_code_widget.rs。首个消费方是 app 侧 ```ai 指令卡：按 info string 判定，与既有链接 block widget 同构，上游可合（注：随 feat(app) commit 一并入库，未拆独立 `vendor:` 前缀 commit） |
| 已提交 | 本轮 | ① | 新增 `SectionAnchor` 与 `section_anchors()`（label.rs）+ 在 `render_galley` 两条分支（可交互 / 不可交互）记录各 section 的顶部 y 到 `ui.data`，测试 tests/section_anchors.rs。动机：`MarkdownLabel` 把内容画进单个 galley，调用方（大纲面板）无从知道某一节落在哪，无法做「滚动到这一节」。纯新增能力，不改动既有渲染路径，上游可合 |
| 待提交 | — | ① | 表格底色能力：`TableStyle` 新增 `header_fill` / `zebra_fill`（默认 false，serde `#[serde(default)]` 向后兼容旧皮肤、Hash 与调试面板同步补齐），`render_table`（table.rs）在 header 各 cell 内容绘制前以 `Visuals::faint_bg_color` 画行底色（取 cell 实际 rect + 与 egui_extras striped 同款 gapless 扩张，横向滚动容器内不越界），body 走 `TableBuilder.striped(zebra_fill)`（egui_extras 内建隔行，同取 faint_bg，与 `vscroll(false)` 无耦合）。测试 tests/table_style.rs（四组合三帧不 panic + faint_bg rect 取证），CHANGELOG [Unreleased] 已记。 LaterMD 侧消费见 #30 T3 |
| 已提交 | 本轮 | ① | 行高从写死 `17px` 常量改为 `MarkdownStyle::line_height_ratio`（默认 1.30）× 各 span 字号：style.rs 新字段（serde default / Hash / UI 三处同步），layout.rs `line_height_for()` 按字号求行高、heading 放大字号后重算——修复 H1（13pt 正文下 20.8pt）被正文行高裁切、折行标题行叠行。测试 tests/line_height.rs，CHANGELOG [Unreleased] 已记。注：最初随 097f69a（app 滚动回归修复）混装入库，2026-09-28 独立评审指出后拆出为独立 `vendor:` commit 并补齐本登记与测试 |
| 已提交 | f8ce3b2 | ① | 行高下限 `min_line_height_em`（#43 M2 CJK 裁切修复）：`MarkdownStyle` 新字段（默认 1.0 = 无下限，serde default / Hash / 调试面板三处同步），`layout.rs` `line_height_for()` 改 `max(字号×ratio, 字号×min_em + 0.75px)`——fallback 字形（CJK ≈1.448em 行 metrics）高于拉丁调校的 ratio（1.30）时，行盒不足会让越界墨迹被相邻行/后续块背景遮挡（"显示不全"）；0.75px 绝对余量覆盖 epaint 行盒整像素吸附的向下取整（最多 0.5px），对所有字号普适。下限只升不降：默认 1.0 在任何常规字号下都不生效，纯拉丁宿主行为逐像素不变。测试 tests/line_height.rs 补 4 例（默认惰性 / floor 抬升 / ratio 高于 floor 不降 / heading 按各自字号生效，3→7 例）。LaterMD 侧消费见 app 的 `theme::effective_markdown_style`（注入本机 CJK face 实际行高）与 fonts.rs 的行 metrics override 副本。注：随 f8ce3b2（fix(app) #43 M2）混装入库，2026-10-02 独立评审指出，待拆出为独立 `vendor:` commit（分支未推送，拆分零风险）；拆分后本行 commit 列改填 vendor: hash |
| 已提交 | 02b33ad | ① | 标题字号分级 + 标题上方呼吸间距（#23 F4，preview-typography §2 待办 A）：`HeadingStyle::scales` 默认 [1.6,1.35,1.2,1.1,1.05,1.0]→[2.0,1.55,1.30,1.15,1.08,1.0]（13pt 正文下 ~26.0/20.2/16.9/15.0/14.0/13.0pt，H4–H6 拉开可辨，H1 2.0× 为中英排版常用档上限）；`MarkdownStyle::heading_space_above`（默认 4.0，serde default / Hash / 调试面板三处同步）以透明 spacer 行（行高 `block_spacing + heading_space_above`）垫在 heading 块首 token 前——落点在 `build_layout` 而非任务书原文的 `render_token_range`（岔路与理由见 decisions-pending #74：纯正文/代码块文档走整篇 galley 路径根本不经过 render_token_range，且两条渲染路径需一处产出才能一致）。整篇 galley 与分段 flush 均生效；文档/段首标题与紧邻块元素的标题不插 spacer；字段归零时 spacer 行恰等于 `block_spacing`（语义完全回落）。测试 tests/heading_spacing.rs 10 例（默认值/字号行高/spacer 三档精确值/每标题线性/开头无 spacer/纯正文零影响否决线/整 galley 路径/segmented 路径/块后保守语义/inline 切片单 spacer）+ egui_markdown_style 内联 serde 兜底测试（旧档缺字段→4.0、显式 0.0 原样保留，dev-dep serde_json）。CHANGELOG [Unreleased] 已记。注：任务书指定的 `render_token_range` 分支未动（`before_block`/`after_block` 原样），正文观感否决线由「纯正文文档 rows 数与高度对新字段不变」测试钉死 |
| 待提交 | — | ① | 分段准入放宽（#77 修复路径第一步，M1）：`MarkdownStyle::segmentation_admission`（默认 `500`，serde default / Hash / 调试面板三处同步照 `heading_space_above` 先例）——fenced 代码块 body 行数达到阈值即按独立 segment 渲染（无需 `scroll_code_blocks`）。准入口径选**单块行数**而非文档 token 数/字节数，理由：`build_layout` 既跑整篇也跑单段 flush 切片，文档级规模在切片内不可知，两处判定输入不同会破坏 `needs_segmentation` 与 `build_layout` 的同判约束（label.rs 整篇路径与 flush 路径两处 `debug_assert!(segment_breaks.is_empty())` 兜底）；块级判定在任何切片上结论一致，且准入不增删 token，跨阈值不挪移后续块序号（widget id 纪律）。改动点：`layout.rs` `code_block_admits_segmentation()` + `needs_segmentation` 增 `style` 参数 + `build_layout` 增 `segment_large_code_blocks: bool`（两处同判，破坏性签名变化已记 CHANGELOG）；`label.rs` `render_token_range` 新分支——admitted fence 作为单 token flush range 渲染（`segment_large_code_blocks=false` 使其内联），复用 flush 的 per-range 缓存与视口剔除，块 widget（表格/图片/滚动代码/`is_block_code_widget` 如 mermaid）的 `try_cull_block` 剔除路径不动。默认 500 保守：常规短文档保持整篇 galley 路径零变化（否决线测试钉死）；长 fence（日志粘贴/流式 LLM 输出）翻入分段路径，追加尾行不再整篇重排。测试 tests/segmentation_admission.rs 6 例 + egui_markdown_style serde 兜底 1 例，既有测试调用签名同步（cache.rs / block_code_widget.rs / heading_spacing.rs / line_height.rs）。LaterMD 侧消费：#46 streaming-perf R2 修复第一步，M2（块级缓存 key）以本模块为前提 |
| 待提交 | — | ① | egui 0.34 → 0.36.2：两个 Cargo.toml 的版本三件套 + pulldown-cmark 0.13.4 + `layout.rs` `ByteIndex`/`ByteRangeExt` 迁移 |
| 待提交 | — | ② | 删 membrane feature：Cargo.toml feature 行、`layout.rs` cfg 块、`style.rs` `InlineCodeStyle` 五个 membrane 字段与 `Stroke` 导入、`tests/indent.rs` 整文件、`tests/width.rs` 的 membrane 测试 |
| 待提交 | — | ③ | 删上游 `[workspace]` 段（两个 Cargo.toml）与嵌套 `rust-toolchain.toml`；Cargo.toml 头部 vendor 注释 |
| 已提交 | 61d7903 | ① | 块级「源区间 → 屏幕 rect」几何通道（#42 大纲跳预览）：新增 `BlockSpanRect` + `block_span_rects()` / `block_rect_at_offset()`（label.rs，lib.rs 导出）——`render()` 每帧重置 `id.with("block-span-rects")` 的帧号键控表，记录点三处：`render_galley` 细块（`is_block_start` 判块界：heading/块级 token/连续两换行；块首行 y 用 `block_needle` 在 job.text 单调正向查找 + `Galley::pos_from_cursor(prefer_next_row)` 定位——不能用 `section_to_token` 反查，epaint `LayoutJob::append` 会合并同格式相邻 section 使两索引不平行）、`render_token_range` 块 widget 粗块（Table/CodeBlock/Image/block widget/blockquote，`try_cull_block` 改返 `Option<Rect>` 供 culling 分支也记）、`flush_text_range` culling 粗块（查表覆盖全文，culling 不留洞）。查询语义：块内命中 / 间隙归最近后续块 / 超文末归最后块；跨帧读返回 `None`（屏幕坐标过期即失效，文档变更随重渲染同帧重建）。配套：`CachedMarkdownLayout` 增缓 `spans`（cache hit 不再丢源区间）。测试 tests/block_span_rects.rs 6 例（混合文档标题单调/四情形/合成表查询语义/同帧重建/跨帧失效/culling 覆盖）。LaterMD 侧消费见 app `ui::preview`（大纲点击 → 源偏移映射 → 查块 → `scroll_to_rect_animation`）。与既有 `SectionAnchor` 并存：行级锚点保留，app 消费已切到块表。注：随 61d7903（feat(app) #42）混装入库，2026-10-03 独立评审指出，待拆出为独立 `vendor:` commit（分支未推送，拆分零风险，先例 f8ce3b2→031fc6f/d686d26 重做）；拆分后本行 commit 列改填 vendor: hash |

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
