# 设计系统规划：尺度 token、强调色纪律与语义正交

> 立项依据：[tobi/disktree](https://github.com/tobi/disktree) 源码审读（2026-10-09）。
> 该项目用 **GPUI + gpui-kit 0.6.6 + gpui-omarchy 0.1.3**，界面观感明显优于
> 同为 egui 的 LaterMD。审读结论：**框架不是原因**。
>
> 本文只规划 LaterMD 侧可迁移的部分。**不引入 GPUI，不改技术栈**（AGENTS.md §2 已否决 GPUI）。
>
> 相关既有文档：`ui-polish.md` §2（token 起点）、`ui-shell-redesign-v2.md` §5.5
> （`RAIL_W` 贴边距教训）、`roadmap.md` 主题专题（token 三层联动模型）。

---

## 1. 诊断：disktree 好看到底好在哪

拆 `crates/disktree-app/src/` 四个文件后，观感来源与框架无关：

| # | 做法 | 源码证据 | 我们的现状 |
|---|---|---|---|
| 1 | **单一 rem 尺度**，全文件无 px | `ui.rs` 全部常量是 `Rems(...)`；间距 7 档 / 字号 6 档 / 图标 3 档；`ZOOM_STEPS` 7 档缩放 | `tokens.rs` 60+ 常量**全是裸 `f32` px**；**无缩放轴** |
| 2 | **色相表写死，饱和明度全类别拉平** | `palette.rs` 的 `const fn hue()` 给 9 分类各锁一个色相角；饱和统一 `0.26`，明度统一 `0.215 + depth*0.028` —— **明度只表达层级，不表达类别** | 无分类色板；`accent` 只服务选中态 |
| 3 | **语义正交，两通道不抢** | 色相 = 「这是什么」；`pattern_slash` 斜线阴影 = 「能不能删」。筛选不命中是 `mix(fill, inset, 0.82)` 淡化，**不换色相** | 选中/警告/危险/成功四色散用 |
| 4 | **只留一个强色** | 琥珀 `theme.warning` 专供「选中 / 主操作 / 可回收」，注释原话 *"One strong colour is kept apart"* | `accent` 已承担部分，但未写成纪律 |
| 5 | **无圆角** | 注释 *"Omarchy's surfaces are square, so nothing in this app rounds a corner"*；所有填充统一 `Corners::default()` | `RADIUS_SM 4` / `RADIUS_MD 6` 已在用 |

**一句话**：它把「好看」从审美选择变成了**写死在两个文件里的常量**——
改不了、也散不掉。我们散在 24 个文件里。

---

## 2. 现状实测（2026-10-09 统计）

### 2.1 裸数字分布

裸 px 字面量出现次数 Top（`crates/latermd-app/src/`，排除 `tokens.rs`）：

| 文件 | 裸数字 | 已引 `tokens::` |
| --- | --- | --- |
| `ui/layout.rs` | 1652 | 41 |
| `ui/minimap.rs` | 1454 | **0** |
| `state.rs` | 1436 | **0** |
| `ui/emoji_panel.rs` | 913 | 9 |
| `ui/editor.rs` | 819 | **0** |
| `ui/sidebar.rs` | 815 | 68 |
| `theme.rs` | 793 | 24 |
| `ui/icons.rs` | 752 | 8 |
| `ui/preview.rs` | 739 | 8 |
| `settings.rs` | 343 | 52 |

**结论**：`tokens.rs` 只被 12 个文件引用，Top 10 里有 5 个引用数为 0。
`minimap` / `editor` / `state` 三个大件完全在 token 体系之外。

> 注：此计数含 `layout.rs` 里的布局计算常量（坐标、rect 数值），
> **不是全部都该 token 化**。§4 给了分流规则。

### 2.2 缺缩放轴

`grep -rn 'ui_scale|zoom|scale_factor'` 全 crate 命中 **0**（仅 `font_metrics_repro.rs`
一处 skrifa 内部的 `px_scale_factor` 注释）。

**当前用户无法整体放大界面**，只能改 `tokens.rs` 里的字号常量并重编译。
roadmap 主题专题提到「字号用户设置另行排队」，但没有缩放轴这个前置。

### 2.3 已知失衡点

`ui-shell-redesign-v2.md` §5.5 记的 `RAIL_W` 48→40 事故，根因是
**度量语境错配**：把「两控件之间的间隔」的量，用在了「控件到窗口边界」上。

这类事故在裸 px 体系里**必然复发** —— 因为没有语义层能表达
「这个 8 是间隔、那个 8 是贴边距」。disktree 的 `space::` 命名
（XXS/XS/SM/MD/LG/XL/XXL）正是为这事准备的。

---

## 3. 目标与非目标

### 目标

1. 间距与字号收敛到**命名语义档**，新增界面元素不再出现裸 px
2. 引入**单一缩放轴**（`ui_scale`），界面缩放保持比例关系
3. 写死**强调色纪律**：一个强调色，只给选中/主操作
4. **明度表达层级**，不用色相表达层级

### 非目标

- 不做每控件样式树（roadmap 主题专题已明确「明确不做」）
- 不做运行时热重载样式引擎（同上）
- 不做皮肤市场（同上）
- 不改 egui 用法、不换框架

---

## 4. 尺度 token：语义档 + 缩放轴

### 4.1 分流规则（先定规则，再搬数字）

**不是所有裸数字都该 token 化。** 三类分流：

| 类别 | 判据 | 处理 |
| --- | --- | --- |
| **界面尺度** | 出现在 Frame margin / item_spacing / 控件宽高 / 字号 | ✅ 进 `space::` / `text::` / `size::` |
| **布局计算** | 参与坐标运算的中间量（`x + w * 0.5`、rect 拆分） | ❌ 留在原处，抽出来反而更难读 |
| **物理量** | 发丝边框（1px）、hairline、treemap 类几何 | ❌ 保持 px，见 disktree 同款理由 |

> disktree `ui.rs` 注释原话同构：
> *"Pixels remain only where a value is physical: hairline borders, the treemap's own geometry, and positions that come from the pointer."*

### 4.2 语义档（对齐 disktree 的 `space::`，按 16px 基准）

```rust
// —— 间距 7 档 ——
XXS: 2.0    // 图标基线、紧凑分隔线
XS:  4.0    // 同一控件内的部件：图标与文字、标题与描述
SM:  8.0    // 紧密关联的控件：按钮组、对话框动作区
MD:  12.0   // 一个内容组：一行的各列、紧凑表单项
LG:  16.0   // 一节内分隔的组 + 区域内边距
XL:  24.0   // 分节
XXL: 32.0   // 大区域边界：空状态呼吸

// —— 字号 6 档 ——
CAPTION: 11.0  // 元信息、tooltip
BODY:    13.0  // 正文与控件标签（现有 Body 13 不动）
SMALL:   14.0  // 现有 FONT_SM 14
TITLE:   16.0  // 窗口/分节/对话框标题
HEADING: 20.0  // 应用名、当前文件名
DISPLAY: 32.0  // 值得从房间对面读出的数字
```

**关键：命名表达「两个东西的关系」，不表达「它今天解析成多少像素」。**
这是 disktree `ui.rs` 注释的意思：
*"Choose a step by what two things mean to each other, not by the pixels it happens to resolve to today."*

### 4.3 「间隔」与「贴边距」必须分档（`RAIL_W` 事故的根治）

`ui-shell-redesign-v2.md` §5.5 的教训要写成结构，而不是注释里的提醒。
新增两个语义命名空间：

```rust
/// 间隔：两侧都是内容。用于控件之间。
pub mod gap { pub const XS: f32 = 4.0; pub const SM: f32 = 8.0; pub const MD: f32 = 12.0; }

/// 贴边距：一侧是内容，另一侧是窗口/容器边界。用于区域内边距。
/// 比同名 gap 紧一档 —— 边界不需要对称留白。
pub mod inset { pub const SM: f32 = 4.0; pub const MD: f32 = 8.0; pub const LG: f32 = 12.0; }
```

`inset::MD(8) < gap::MD(12)` 是刻意的。这样「贴边」和「间隔」在代码里
**长得不一样**，下次不会再拿同一个常量去填两个位置。

### 4.4 缩放轴

```rust
/// 界面缩放档（相对 16px 基准 rem）。参考 disktree 的 ZOOM_STEPS。
pub const SCALE_STEPS: [f32; 7] = [0.75, 0.875, 1.0, 1.125, 1.25, 1.5, 1.75];

/// 当前缩放。由 Settings 写入，经 theme 投影进 egui Style。
pub fn ui_scale() -> f32 { /* 读 setting，默认 1.0 */ }
```

投影点只有三处（与 `theme::apply_shell` 同一入口，不新增散点）：

| 目标 | 投影字段 |
| --- | --- |
| egui `Style` | `spacing.item_spacing`、`spacing.button_padding`、`spacing.interact_size` |
| egui `TextStyle` | `Body` / `Small` / `Heading` 字号 |
| 本仓 `size::*` 消费点 | 统一经 `fn px(t: f32) -> f32 { t * ui_scale() }` |

**不做**运行时重排：egui 立即模式下改 token 即同帧生效，
这与 roadmap「热重载引擎是伪需求」的判断一致。

### 4.5 验收

- `space::` / `text::` / `size::` 三档集各有守门测试，断言「档位单调递增且非退化」
- 缩放 0.75 / 1.0 / 1.75 三档下，`rail`+`nav`+`内容区` 无重叠、无溢出（沿用现有 `layout.rs` harness）
- `ui/layout.rs`、`ui/sidebar.rs` 里**界面尺度类**裸 px 归零（布局计算类不动）

---

## 5. 强调色纪律：一个强色，其余退让

### 5.1 现状问题

`tokens.rs` 有 `accent`（蓝）、`WARN`（黄）、`DANGER`（红）、`OK`（绿）、
`rail_fill`、`rail_divider`。前四个都是**语义色**，但没有一条规则说
「什么时候该用强调色」。实际代码里 `accent` 的用法是「页签选中 + 选中态下划线 + 主按钮」
—— 已经是 disktree 那种用法，但**没写下来**，下一个模块会扩出去。

### 5.2 纪律（写进 `tokens.rs` 文档注释 + 守门测试）

| 规则 | 内容 |
| --- | --- |
| 1 | `accent()` **只给三类**：当前选中态、主操作按钮、焦点环 |
| 2 | **警告/危险/成功不占用 accent 档**（已经是三个独立色，不合并） |
| 3 | AI 专属元素继续用紫罗兰（现状已如此，见 `ui::preview::ai_link_color`）——「唯一用紫罗兰的东西」反而更醒目 |
| 4 | 任何新增 `accent()` 调用点，必须能说出「它属于三类里的哪一类」 |

### 5.3 明度表达层级，不用色相表达层级

disktree 最值得抄的一条：**明度只编码深度**。
`palette.rs` 里 9 个分类饱和度全部 `0.26`，明度全部 `0.215 + depth*0.028` ——
色相之间可以差很远（0.065 ~ 0.955），但**没有任何一块颜色因此显得更重要**。

对我们的直接含义：**侧栏文件树不要给不同层级/类型不同色相。**
深度用缩进 + 字重表达，类型用图标表达，悬停/选中才上 `accent`。

### 5.4 验收

- 守门测试：全仓 `accent()` 调用点逐个归类到三类之一，编译期无法自动核对的写成清单随测试同文件
- 侧栏树在无 accent 时（截图把 accent 全去掉）仍能靠缩进/字重读出层级 —— 这是硬判据

---

## 6. 语义正交：颜色不背多个含义

disktree 把两个维度拆开：色相 = 「这是什么」，斜线阴影 = 「能不能删」。

### 6.1 对我们的映射

| 维度 | 承载 | 不承载 |
| --- | --- | --- |
| **色相** | 语义类别（代码/缓存/git/媒体 —— 若将来做 minimap 着色） | 状态（选中/警告） |
| **强调色** | 选中态、主操作 | 类别区分 |
| **字重 / 缩进** | 树层级 | — |
| **图标** | 类型、状态（脏标记、未推送） | — |
| **底色深浅** | 层级深浅 | — |

### 6.2 具体动作

- `ui/minimap.rs`（1454 裸数字、0 token 引用）若引入着色，**必须**按上表分配通道，
  不允许「一个颜色同时表示选中 + 大文件」
- 筛选/搜索命中与未命中：用**淡化**（往背景色 mix）而非换色相 ——
  disktree `mix(fill, inset, 0.55 / 0.82)` 两档
- 状态语义继续走图标 + 文字（脏标记、未推送数），不走底色

---

## 7. 门禁与排期

### 7.1 六项门禁（沿用 AGENTS.md §8，本专题额外加第 7 项）

1. `cargo fmt --all --check`
2. 三轮 `cargo clippy --workspace --all-targets`（default / `--no-default-features` / `--all-features`，均 `-D warnings`）
3. `cargo test --workspace --all-features`
4. `cargo doc --no-deps --all-features`
5. vendor 改动时 `vendor/egui_markdown/check.sh`
6. 新增守门测试做**红绿验证**（临时改坏实现看是否真红）
7. **本专题专用**：`tokens.rs` 档位单调性测试 + `accent()` 调用点清单测试

### 7.2 批次

| 批次 | 内容 | 落点 | 验收 |
| --- | --- | --- | --- |
| **T1** | `space::` / `text::` / `gap::` / `inset::` 四档集落地，**只新增不迁移** | `ui/tokens.rs` | 档位单调测试绿；零调用点改动 |
| **T2** | `ui/layout.rs` + `ui/sidebar.rs` 界面尺度类裸 px 迁移（按 §4.1 分流） | 同上 + 两文件 | §4.5 三条全绿；两文件裸 px 计数降 80%+ |
| **T3** | `ui_scale()` 缩放轴 + 三处投影 + Settings 条目 | `theme.rs` + `settings.rs` | 0.75/1.0/1.75 三档无重叠溢出 |
| **T4** | `accent()` 纪律成文 + 调用点清单 + 侧栏树去色相验证 | `tokens.rs` 文档 + 守门测试 | §5.4 两条 |
| **T5** | `ui/minimap.rs` / `ui/editor.rs` 归流（体量最大，单独排） | 两文件 | 各自裸 px 归零或给出不迁移的分类清单 |

T1 是纯增量、零风险，先落。T5 体量最大且 `minimap` 涉及渲染热点，
排在缩放轴之后（先有 T3 的缩放轴，T5 才不会把裸 px 迁移和缩放两件事搅在一起）。

### 7.3 明确不做

- 主题热重载引擎（roadmap 已判为伪需求）
- 每控件粒度样式树（roadmap 已明确）
- 把 `layout.rs` 的布局计算量也塞进 token（会让计算更难读，见 §4.1）
- 为 minimap 之外的新模块预建色板（`palette.rs` 的 9 分类是 disktree 的**领域**特例，
  通用色板抄过来只会有一堆用不上的槽位）

---

## 8. 待确认

1. **T2 的分流边界**：`layout.rs` 1652 个裸数字里，「界面尺度」与「布局计算」的切分
   需要逐处判断。这是一次需要判断力的重构，不适合机械执行 —— 建议先做
   `ui/sidebar.rs`（815 个、有 68 处已引 token，边界较清晰）验证方法，再推 `layout.rs`。
2. **字号与缩放的关系**：`text::BODY 13` 是默认档，用户设的字号设置
   （roadmap 专题提到的「另行排队」）应作用于 `text::*` 的基准还是覆盖全部档位？
   本规划按「基准 + 等比缩放」写，若要「只改正文不动标题」需另立决策。
3. **圆角要不要也收档**：disktree 全平直角，我们已有 `RADIUS_SM/MD`。
   是否进一步统一到单档？本规划不动，只登记为观察项。

---

## 9. 关联风险

- **T2/T5 的重构风险高于新增**。守门测试密度不足时，大规模搬数字容易「跑绿了但搬漏了」。
  对策：每批都做红绿验证 + `git diff --numstat` 核删除行数
  （沿用 `ui-shell-redesign-v2.md` §2.1 纪律）。
- **缩放轴会牵动布局缓存**：`theme.rs` 已把 `dark_mode` / `code_theme` 计入布局缓存
  hash。引入 `ui_scale` 后**必须**一并计入，否则缩放时正文/表格布局会命中旧缓存。
  这是 T3 最容易漏的一处。
