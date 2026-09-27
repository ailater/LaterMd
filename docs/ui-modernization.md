# UI 现代化路径评估（ui-modernization）

日期：2026-09-27
状态：**已裁决**（五条建议，1 采纳 / 1 待决 / 3 否决）
关联：[AGENTS.md](../AGENTS.md) §2 技术栈 / §5 渲染后端 / §6 vendor、
[ui-polish.md](ui-polish.md)、[ui-shell-redesign.md](ui-shell-redesign.md)、
[adr-004](adr-004-technical-stack.md)

> 坤哥 2026-09-27 转来一份「对 LaterMD 的推荐实施路径」P0–P4 五条。
> 本文**逐条查证后再裁决**，不照抄——五条里有三条踩我们的硬约束，两条是许可证/版本级冲突。

---

## 1. 裁决总表

| 原优先级 | 建议 | 裁决 | 一句话理由 |
|---|---|---|---|
| P0 | `egui-thematic` 替换默认 Visuals | ❌ **否决** | 27 次总下载 + 锁 egui **0.33**（我们是 0.36.2）+ 与自有主题/皮肤系统重复 |
| P0 | 启用 Inter 字体 | ✅ **采纳** | 可行，但**必须挂 CJK fallback**，否则中文变豆腐块 |
| P1 | `egui-phosphor` 换掉文字按钮 | ⚠️ **待坤哥拍板** | 库本身健康，但要推翻「图标自绘」铁律，且 30+ 自绘图标返工 |
| P2 | `Armas` 重构核心控件 | ❌ **否决** | 23 下载/月 + 锁 egui 0.33 + 会推翻 M1/M2/M3 全部自绘控件 |
| P3 | `backdrop-blur-egui` 毛玻璃 | ❌ **否决** | 需要 **glow**（AGENTS §5 明令不启用）或不用 eframe（我们整个外壳是 eframe） |
| P4 | `egui_transition_animation` 动效 | ❌ **否决** | **GPL-3.0-or-later**，LaterMD 是 MIT，直接不合规 |

---

## 2. 逐条查证

### 2.1 `egui-thematic` — ❌ 否决

| 指标 | 实测（2026-09-27） |
|---|---|
| 版本 / 时间 | 0.1.1，**9 个月前** |
| 下载量 | **27 次总下载** |
| egui 依赖 | **0.33.0** ← 我们是 0.36.2 |
| 作者 | 单人；keywords 里带 `nightshade`（他自己的游戏引擎） |
| 体积 | 28 KiB / 2.3K SLoC |

否决理由（三重）：
1. **版本错配**：AGENTS §2 明写「五个子 crate 同步发版，无版本错配」。引入它会拉出第二份 egui 0.33，
   与 `eframe 0.36.2` 的 `Visuals` 类型不是同一个类型——`ctx.set_visuals()` 根本收不下它的产物。
2. **双主题系统**：我们已有 `theme.rs` 三态（亮/暗/跟随系统）+ `themes/*.ron` 皮肤文件
   （批次 B 已交付）。再挂一套 = 用户改 A 处不影响 B 处，是 bug 不是功能。
3. **成熟度**：27 次下载、9 个月不动，与 AGENTS §2 已否决的 `tektite`（146 下载 / 单人业余）同款画像。

**白拿它产出的办法**：它的 9 套预设（Dracula / Nord / Gruvbox / Solarized / Tokyo Night …）
本质是**一组颜色数值**。把这些色板抄成 `themes/*.ron` 皮肤文件即可——纯数据、零依赖、不吃它的代码。

### 2.2 Inter 字体 — ✅ 采纳（带前提）

可行。但 **Inter 没有中文字形**，裸挂会让中文全变豆腐块——AGENTS §5 把 CJK 字体列为头号风险不是说着玩的。

正确做法：
- Inter 挂在 `FontDefinitions` 的 **Proportional 首位**，我们已有的三平台 CJK 候选表
  （P0-fixes 已建）作为 **fallback** 跟在后面。
- egui 的字体 fallback 是逐字形回退，混排可用。
- **必须实测**：Inter 与 CJK 混排时的基线对齐和行高（两者 ascent/descent 不同），
  这是 M5 像素验收要加的一项。

顺带：把 Inter 的 **Medium / SemiBold** 两个字重也挂上——工具条与标题用 SemiBold，
是「精致度」最便宜的来源，比换整个控件库划算得多。

### 2.3 `egui-phosphor` — ⚠️ 待坤哥拍板（库是健康的，代价是返工）

| 指标 | 实测 |
|---|---|
| 版本 / 时间 | 0.14.0，**14 天前**（2026-09-10，bump to egui 0.36） |
| 下载量 | ~2900/月，**#65 in GUI** |
| egui 依赖 | **0.36** ✅ 匹配 |
| 许可 | MIT / Apache-2.0（Phosphor Icons 本体 MIT） |
| 体积 | 1.03 MiB（bundled 字体 + 可选 subset 裁剪） |

**必须先修正一条旧结论**：ui-polish §1.1「图标一律自绘、零字体依赖」的论据是
「emoji/Unicode 在三平台缺字」。这个论据**对 phosphor 不成立**——它是把字体文件**嵌进 crate**
（`add_to_fonts` 塞的是 bundled bytes），不依赖系统装了什么字体。
所以「字体图标 = 有缺字风险」在这里是错的，铁律需要按「系统字体 vs 内嵌字体」细分。

但仍然**不在本轮做**，理由：
1. 我们已有 **30+ 枚自绘图标**（`ui/icons.rs`：文件组/格式组/标题栏/侧边栏/状态），
   且 LOGO 也是自绘同源（`assets/logo/`）。自绘这套已经过明暗双主题与像素验收。
2. **两套图标并存 = 视觉分裂**，这是所有选项里最糟的一个。要么全换，要么不换。
3. 全量迁移要动工具条 17 枚 + 文件栏 + 侧边栏 + 标题栏六按钮 ≈ **2–3 天**，不是原表写的「低」。

**给坤哥的两个选项**：
- **甲（推荐）**：本轮不动。精致度走 §3 的 U0/U1（改 token + 换字体），成本 1 天，风险 0。
- **乙**：单开一棒「图标全量迁移 phosphor」，`icons.rs` 退役，与图片/图床主线**串行**做。

### 2.4 `Armas` — ❌ 否决

| 指标 | 实测 |
|---|---|
| 版本 / 时间 | 0.2.2，**4 个月前**（2026-05-14）；`armas-basic` 0.2.0 锁 egui **0.33** |
| 下载量 | **23 次/月**，#1587 in GUI |
| 体积 | 6K SLoC（umbrella），`armas-basic` 16K SLoC |
| 许可 | MIT / Apache-2.0 ✅ |

否决理由：
1. **版本落后**：`armas-basic` 锁 egui 0.33，我们是 0.36.2，同 §2.1 第 1 条。
2. **工作量被严重低估**：原表写「中」。"重构核心控件（按钮、输入框、侧边栏、Tab）"
   意味着推翻 **M1 自绘标题栏 + M2 手绘 `icon_label_row` + M3 自绘工具条图标 + LOGO 自绘体系**，
   还要重接 `Panel::left/right` 的三栏分工（adr-005 §3.2）。这是**数周**，不是"中"。
3. **成熟度**：23 次/月 ≈ 除作者外无人用。拿它当全应用控件基座，等于把产品地基押在单人项目上。

**白拿它产出的办法**：它的价值在**设计数值**不在代码。可直接抄进 `tokens.rs`：

| 抄什么 | armas 里的值 | 落点 |
|---|---|---|
| 控件圆角 | 6.0（rounded-md） | `RADIUS_MD` |
| 输入框高度 | 36.0（h-9） | 新 `INPUT_H` |
| 输入框内边距 | x=12 / y=8（px-3 / py-2） | 新 `INPUT_PAD_X/Y` |
| 正文小字号 | 14.0（text-sm） | 新 `FONT_SM` |

零依赖拿到 shadcn 80% 的观感。

### 2.5 `backdrop-blur-egui` — ❌ 否决（与渲染后端策略硬冲突）

| 指标 | 实测 |
|---|---|
| 版本 / 时间 | 0.2.0，0.1.0 是 **8 天前**首发（pre-release，作者明说"pin an exact version"） |
| egui 依赖 | **^0.34** ← 我们是 0.36.2 |
| wgpu | **^29** |
| 作者 | 单人 |

三条硬冲突，任何一条都足够否决：
1. **grab-pass 路径需要 glow 后端**。AGENTS §5 明写「**不要启用 glow**」——macOS 已废弃 OpenGL，
   我们全平台走 wgpu，只有 `LATERMD_RENDERER=glow` 这一个驱动黑名单逃生口，不能为毛玻璃把它变成常规路径。
2. **own-loop 路径要求不用 eframe**，直接驱 `egui-winit` + `egui-wgpu`。我们的整个外壳（标题栏、
   三栏、`layout.json`）都建在 eframe 之上，改这个等于重做外壳。
3. **我们是无边框自绘窗口**（D1 已拍板）。没有 OS 层的 vibrancy / Acrylic 材质可借，
   毛玻璃只能靠"抓帧 + 模糊 + 合成"自己画——为一个二级视觉效果付这个代价不值。

**替代**：要层次感，用**半透明面板底色 + `WindowShadow`** 即可，零依赖。

### 2.6 `egui_transition_animation` — ❌ 否决（许可证）

**GPL-3.0-or-later。** LaterMD 是 MIT（`LICENSE`：Copyright (c) 2026 crazykun）。
引入 GPL 依赖会强制整个项目转 GPL 或做隔离分发——**直接不合规，这条不需要看别的指标**。

（顺带：GitHub 1 star、351 次总下载、252 SLoC、12 个月未更新。也没什么可惜的。）

**替代**：egui 自带 `style.animation_time`。编辑/预览切换要淡入，
`ctx.request_repaint()` + 自己插值一个 0.15s 的 alpha 就够，**约 20 行**，无依赖。

---

## 3. 我推荐的实施路径（替换原 P0–P4）

原则：**能抄数值的不引库，能自研 20 行的不引依赖，要引的必须是 MIT/Apache 且 egui 版本对齐。**

| 序 | 内容 | 成本 | 依赖 | 备注 |
|---|---|---|---|---|
| **U0** | 扩 `tokens.rs`：圆角/间距/字重/控件尺寸（抄 armas 数值）+ 预设色板转 `themes/*.ron`（抄 egui-thematic 色板） | 0.5d | **零** | 最大性价比，先做这个 |
| **U1** | Inter（Regular/Medium/SemiBold）+ CJK fallback | 0.5d | Inter 字体文件 | 混排基线要进 M5 像素验收 |
| **U2** | **图片框 + 图床**（见 [image-plan.md](image-plan.md)） | 4d | `ureq` multipart | 坤哥本轮主线 |
| **U3** | 动效：自研 20 行插值（编辑/预览切换、浮层淡入） | 0.2d | **零** | 替代 P4 |
| **U4** | phosphor 全量迁移 —— **待坤哥在甲/乙之间拍板** | 0 或 2–3d | `egui-phosphor 0.14` | 见 §2.3 |

U0–U3 合计约 5.2d，**全部零新增第三方依赖**（Inter 是字体资源不是库）。

---

## 4. 一条需要修订的既有结论

`ui-polish.md` §1.1「图标是矢量自绘，不是字体字符」的**论据**需要细分：

| 类型 | 缺字风险 | 结论 |
|---|---|---|
| 系统 emoji / Unicode 符号（✎ 🗋 ⌘） | **有**（三平台字体不同） | 原结论成立，**继续禁用** |
| 内嵌字体的图标库（phosphor，bundled bytes） | **无**（字体随 crate 分发） | 原论据**不适用**，不能拿它否决 phosphor |

裁决 phosphor 应基于「返工成本 / 视觉一致性」，而不是「字体依赖风险」。
这条已同步记入 [decisions-pending.md](decisions-pending.md)。
