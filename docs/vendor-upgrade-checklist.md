# egui_markdown vendor + 升级操作清单

状态: **已执行(2026-09-24,六项门禁全绿)**
估时: **3-5 个工作日**（不含 `#membrane` feature 部分）
实测环境: rustc 1.98.0, `/tmp/em_head` = `membrane-io/egui_markdown` HEAD

> **执行结果纪要(2026-09-24)**:15 个真实错误全部按本文修法解决;`membrane` feature 已整
> 体删除(23 处 cfg,含 style crate 的 5 个字段、tests/indent.rs、tests/width.rs 一个测试);
> `Token::source_span` 以平行数组落地(`Markdown { s, tokens, spans }`)。**两处本文未预见的
> 问题**:① `ByteIndex` 需在 `mod syntect_code` 内单独 import(顶层 use 不进子模块);②
> egui 0.36 新增 `TexturesDelta` drop 检查,10 个测试因无渲染环境下 drop 未消费的
> font atlas delta 而 panic,修法是接住 `run_ui` 的 `FullOutput` 并在 drop 前
> `output.textures_delta.clear()`。全部差异见 vendor/egui_markdown/README.md。

---

## 0. 为什么需要这份清单

`egui_markdown` 的 **crates.io 发布版停在 0.1.0（依赖 egui 0.33）**，而 GitHub HEAD 已升到 **egui 0.34**。本项目需要 **egui 0.36.2**。

同一二进制里不能有两个 egui 版本（type 不兼容），因此必须 vendor 源码并自行升级。

---

## 1. Vendor 步骤

```bash
cd LaterMD
git clone https://github.com/membrane-io/egui_markdown.git vendor/egui_markdown
cd vendor/egui_markdown
rm -rf .git            # 或用 git subtree，见下方决策
cat rust-toolchain.toml
```

### 必须保留的文件（不要删）

| 文件 | 原因 |
|---|---|
| `egui_markdown_style/` | workspace 第二个 crate，`MarkdownStyle` 所在，**含 serde feature** |
| `tests/` | `cache.rs` / `indent.rs` / `truncate.rs` / `width.rs` 四个集成测试 |
| `check.sh` | 六项质量门禁，照抄到 CI |
| `LICENSE-MIT` + `LICENSE-APACHE` | 双许可要求 |
| `DESIGN.md` / `ECOSYSTEM.md` | 设计意图文档，改动前必读 |

### `git clone` vs `git subtree` 的选择

| 方式 | 优点 | 缺点 |
|---|---|---|
| `git clone` + 删 `.git` | 简单，改动自由 | 丢失上游历史，将来合上游 patch 麻烦 |
| `git subtree add` | 保留上游历史，可 subtree pull 合 patch | 命令略复杂，diff 噪音大 |

**推荐 `git subtree`**，理由：上游活跃（13 天内仍有提交），未来大概率要 merge 他们的修复。

```bash
git subtree add --prefix=vendor/egui_markdown \
  https://github.com/membrane-io/egui_markdown.git main --squash
```

### 修改 `rust-toolchain.toml`

上游写的是 `channel = "stable"`，**必须改为钉死**：

```toml
[toolchain]
channel = "1.98.0"
```

因为 egui 0.36.2 要求 rustc ≥ 1.95（本机默认 1.94.0 编译失败，已实测）。

---

## 2. 版本依赖修改清单

`Cargo.toml`（主 crate）：

```toml
egui            = { version = "0.36.2", default-features = false }
epaint          = { version = "0.36.2", default-features = false }
egui_extras     = { version = "0.36.2", default-features = false }
eframe          = { version = "0.36.2", default-features = true, features = ["default_fonts", "glow"] }  # dev-dep
egui_extras     = { version = "0.36.2", features = ["syntect"] }                                          # dev-dep
```

`egui_markdown_style/Cargo.toml`：

```toml
egui            = { version = "0.36.2", default-features = false }
```

---

## 3. 实测结果：两组工作量

| 构建配置 | 错误数 | 说明 |
|---|---|---|
| `cargo build`（默认 features） | **15** | **这是我们要走的路** |
| `cargo build --all-features` | **24** | 多出的 9 个全部来自 `membrane` feature |

> **决策建议：不启用 `membrane` feature。** 那 9 个错误涉及 `LeadingSpace::Indent` / `LeadingSpace::FirstRow`（0.36 已彻底移除，见 §5.1）以及 `TextFormat::bg_stroke` / `bg_corner_radius`（仅 membrane 分支存在），是上游为他们自家产品做的定制，对我们无价值。**省掉 9 个错误里最难的那批。**

---

## 4. 15 个错误的分类修复指南

### 4.1 A 组：`Range<ByteIndex>` 不能再索引 `str`（4 处）

**根因**：egui 0.35 PR #8245 引入强类型索引。`LayoutSection::byte_range` 类型从 `Range<usize>` 变为 `Range<ByteIndex>`。

**官方提供的 escape hatch**：`ByteRangeExt::slice()` 和 `ByteRangeExt::as_usize()`

```rust
use epaint::text::ByteRangeExt as _;

// 修法：直接用 slice()
let s = section.byte_range.slice(&job.text);

// 或转成 usize range
let s = &job.text[section.byte_range.as_usize()];
```

**受影响位置**：

| 文件:行 | 现写法 | 改法 |
|---|---|---|
| `layout.rs:49` | `&job.text[section.byte_range.clone()]` | `section.byte_range.slice(&job.text)` |
| `layout.rs:406` | `&highlighted_job.text[section.byte_range.clone()]` | `section.byte_range.slice(&highlighted_job.text)` |
| `label.rs:1118` | `galley.job.text[s.byte_range.clone()]` | `s.byte_range.slice(&galley.job.text)` |
| `label.rs:1124` | `galley.job.text[...byte_range.clone()]` | 同上 |

> **难度：低。机械替换，但有 1 处（label.rs:1123）需要额外处理，见 B 组。**

### 4.2 B 组：`Range` 不再有 `.len()`（1 处）

`label.rs:1123`：

```rust
let sec_char_count = galley.job.sections[sec_idx]
    .byte_range
    .clone()
    .len()                                    // ❌ Range<ByteIndex> 无 len()
    .min(galley.job.text[...].chars().count());
```

**这段代码的意图是取「字节长度」和「字符数」的较小值 —— 这本身就是 bug**（混用了 byte 与 char 两种单位）。升级时建议直接重写为：

```rust
let section_text = galley.job.sections[sec_idx].byte_range.slice(&galley.job.text);
let sec_char_count = section_text.chars().count();
let sec_end_char = sec_start_char + sec_char_count as u32;
```

> **难度：低，但需判断原意图。注意 `slice()` 已保证不越界，`.min()` 保护可去掉。**

### 4.3 C 组：`LayoutSection.leading_space` 类型变化（4 处）

`layout.rs:639` 和 `layout.rs:658`，每处两个错误（第 21/33 列、19/31 列）。

现写法：

```rust
job.sections.push(LayoutSection {
    leading_space: 0.0_f32.into(),     // ❌ 0.36 中 leading_space 已是 f32，无 From<f32> for f32 的 into()
    byte_range: byte_start..byte_end,  // ❌ 需要 ByteIndex
    format: TF { .. },
});
```

改法：

```rust
use epaint::text::ByteIndex;

job.sections.push(LayoutSection {
    leading_space: 0.0,
    byte_range: ByteIndex(byte_start)..ByteIndex(byte_end),
    format: TF { .. },
});
```

> **难度：低。纯机械改动。**

### 4.4 D 组：`Row::char_count_including_newline()` 返回 `CharIndex`（5 处）

**根因**：PR #8245 把返回值从 `usize` 改成了 `CharIndex`。

#### `paint.rs` 的三处

现写法：

```rust
let mut offset = 0;
for (row_index, row) in galley.rows.iter().enumerate() {
    let row_range = offset..=offset + row.char_count_including_newline();  // ❌ 0 + CharIndex
    ...
    offset += row.char_count_including_newline();                          // ❌ usize += CharIndex
}
```

改法（**最干净**）—— 把 `offset` 的类型改成 `CharIndex`：

```rust
use epaint::text::CharIndex;

let mut offset = CharIndex::ZERO;
for (row_index, row) in galley.rows.iter().enumerate() {
    let row_end = offset + row.char_count_including_newline();
    let row_range = offset..=row_end;
    ...
    offset = row_end;
}
```

> `CharIndex` 实现了 `Add<usize>` 和 `AddAssign`（见 egui `index.rs` 的 `arithmetic` test），所以 `offset + count` 和 `+=` 都合法。

**受影响位置**：`paint.rs:19`、`paint.rs:31`、`paint.rs:68`

#### `label.rs` 的三处（`E0605: non-primitive cast CharIndex as u32`）

现写法：

```rust
index += row.char_count_including_newline() as u32;   // ❌ 非原生 cast
```

改法：

```rust
index += u32::try_from(row.char_count_including_newline().0)
    .unwrap_or(u32::MAX);
```

或直接把 `index` 也改成 `CharIndex` 类型（如果调用方能接受签名变更，**这是更彻底的方案**）。

**受影响位置**：`label.rs:1363`、`1372`、`1375`

> **难度：中。需要沿调用链确认类型传播范围，`last_non_whitespace_glyph` / `cursor_from_pos` 的公共签名可能要跟着改。**

---

## 5. 不启用的部分（`membrane` feature 的 9 个错误）

### 5.1 `LeadingSpace` 枚举已被完全移除

`layout.rs:391/446/450` 使用：

```rust
job.push_with_leading_space("", epaint::text::LeadingSpace::Indent(indent), format);
```

实测：egui 0.36.2 中 **`LeadingSpace` 枚举和 `LayoutJob::push_with_leading_space()` 方法都不存在**。`LayoutSection::leading_space` 退化为普通 `f32`（第一行的缩进），不再支持「换行后续行的悬挂缩进」。

**这是 0.36 的能力回退**，不是 API 改名。`#[cfg(not(feature = "membrane"))]` 分支里上游已经写了注释承认这点：

```rust
// Upstream egui only supports first-row leading space; wrapped rows return to column 0.
```

**对我们的影响**：代码块长行换行后，续行会从第 0 列开始而非对齐缩进。可接受（属于上游已知限制）。

> **行动：确保 `membrane` feature 不启用，让这段死代码被 cfg 掉。**

### 5.2 `TextFormat` 的 `bg_stroke` / `bg_corner_radius`

`layout.rs:67/68` 在 `#[cfg(feature = "membrane")]` 分支内引用这两个字段。实测 egui 0.36.2 的 `TextFormat` **只有 `expand_bg: f32`**，没有那两个。

随 §5.1 一起被 cfg 掉即可。

---

## 6. 验收标准

改完后必须全绿：

```bash
cd vendor/egui_markdown

cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --no-default-features -- -D warnings
cargo clippy --all-targets --all-features -- -D warnings   # 若保留 membrane，此项会失败
cargo test --all-features
cargo doc --no-deps --all-features
```

**建议**：既然决定不启用 `membrane`，可在 vendor 时直接从 `Cargo.toml` 删掉该 feature 及其 data-defined 代码（grep `#[cfg(feature = "membrane")]` 共约 5 处），避免死代码长期腐化。这样 CI 里 `--all-features` 也能过。

> 删除上游 feature 属于破坏性改动，请在 `vendor/README.md` 里记录删除理由，便于将来 merge upstream patch 时识别冲突。

---

## 7. 工作量再核对

| 组 | 处数 | 估时 |
|---|---|---|
| A 组 `byte_range.slice()` | 4 | 0.5 小时 |
| B 组 重写逻辑 | 1 | 1 小时（含判断原意图） |
| C 组 `leading_space` / `ByteIndex` | 4 | 0.5 小时 |
| D 组 `CharIndex` 传播 | 6 | **0.5–1 天**（需沿调用链改类型） |
| 删除 `membrane` feature | ~5 处 cfg | 1 小时 |
| 跑通 check.sh 六项 | — | **1–2 天**（clippy pedantic + nursery 很严） |
| 合计 | | **3–5 工作日** |

**最大的不确定项是 D 组和 clippy。** D 组若发现 `CharIndex` 需传播到公共 API（`cursor_from_pos` 返回 `Option<u32>`），改动面会扩大；clippy `-D warnings` 在 `pedantic + nursery` 下通常要清理几十处。

---

## 8. 建议的执行顺序

1. 先做 A/B/C 三组（机械改动，约 2 小时），把错误数从 15 降到 6
2. 集中攻 D 组（`CharIndex` 类型传播）
3. 删除 `membrane` feature 相关 cfg
4. 跑 `cargo test` 验证行为未回归（这是最重要的一步 —— **byte/char 混用的强类型重构最容易引入 off-by-one**）
5. 最后才是 `cargo fmt` + `clippy`

> **第 4 步不能跳过。** 现有的 `tests/*.rs` 就是为此存在的。

---

## 9. 提交拆分与上游回馈（规范）

> 规范正文在 [AGENTS.md](../AGENTS.md) §6 第 9 条与 [vendor/README.md](../vendor/README.md)（变更登记处，subtree 前缀之外）。本节只记操作。

### 9.1 三类拆分

vendor 改动 commit 按主题拆：①上游可合（升级 / API 迁移 / 通用能力）②私有删改（membrane）③仓库接驳（workspace / toolchain）。同一文件混多类（如 `layout.rs` 同时含 `ByteIndex` 迁移与 membrane cfg 删除）时按 hunk 拆：

```bash
git add -p vendor/egui_markdown/src/layout.rs   # 逐 hunk 归入不同 commit
```

同一 hunk 混两类时归 ①；提交后在 vendor/README.md 登记表补 hash。

### 9.2 发往 fork 的提取命令

```bash
# 主仓导出 ① 类累计改动（基点 = subtree merge commit，或上一个 ① 类 commit）
git diff 0fe1510 -- vendor/egui_markdown > /tmp/upgrade.patch

# 在 fork clone 里应用（去掉 vendor/egui_markdown 两级前缀）
git apply -p2 /tmp/upgrade.patch
```

随后手工补齐 membrane 分支的 0.36 迁移（`LeadingSpace` 悬挂缩进、`TextFormat::bg_stroke`/`bg_corner_radius` 在 0.36 无直接等价，需实现或向作者说明取舍），过上游 `./check.sh`，更新 CHANGELOG，再发 PR。

### 9.3 上游地址与 CONTRIBUTING 硬要求

- **canonical：`membrane-io/egui_markdown`**。`iamseeley/egui_markdown` 301 跳转到此（仓库已转移到组织）；上游 Cargo.toml 的 `repository` 字段是转移前旧地址，勿据此判断。截至 2026-09-24 上游 main 仍在 egui 0.34。
- 一个 PR 单一变更；新 parser/layout 行为必须带测试；`CHANGELOG.md` 加 `[Unreleased]`；`./check.sh` 全绿。
- 风格：行内注释以句号结尾、无 banner、`use` 排序、`format!` 内不写 `.to_string()`、doc 注释在 `#[allow]` 之前。
