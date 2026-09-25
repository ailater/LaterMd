# 界面设计规范（UI Design Spec）

> **一句话**：这份文档做两件事 —— ① 教你（和 Agent）把「这 UI 太丑」翻译成工程能直接执行的参数；② 给出唯一一套视觉终态（token + 组件规范 + 验收）。
>
> 状态：已接受（2026-09-25）
> 定位：这份是 [docs/roadmap.md](roadmap.md)「专题：界面美化与皮肤系统」**批次 B 的视觉细化**，不新增范围、不改排期口径。周期与阶段归属仍以 roadmap 为准。

---

## 0. 边界（先说不做什么，避免范围蔓延）

| 项 | 规则 |
|---|---|
| 改动范围 | 仅 `crates/latermd-app/src/theme.rs` 与 `crates/latermd-app/src/ui/*` |
| **不碰 vendor** | `vendor/egui_markdown/` 一行不改。正文层通过 `MarkdownStyle` 扩展点投影，这是 roadmap 专题已实测的既有能力（`color_dark`/`color_light` 成对设计 + `code_theme` 扩展点 + 缓存 hash 已含 `dark_mode`） |
| **不碰其他 crate** | `latermd-md` / `latermd-editor` / `latermd-export` 不依赖 UI 框架（AGENTS.md 铁律 2），视觉改动一律不上浮 |
| 不引依赖 | 不加字体库、不加图标库、不加样式引擎。图标用 egui 内建 `egui::special_emojis` 或自绘，字体沿用 `fonts.rs` 系统回退 |
| 不做 | 皮肤市场、CSS 式主题引擎、每控件自定义样式树、True WYSIWYG（AGENTS.md §7 已否决） |

**核心约束：token 只有一处事实源。** 落地后 `crates/latermd-app/src/` 下不允许再出现裸色值（`Color32::from_rgb`）与裸间距（`add_space(4.0)` 之外的 magic number），一律引用 token 常量。这条进 CI 靠人工 review 把关，不写脚本。

---

## 1. 怎么把「丑」说成工程能执行的话

这是本文档最该先读的一节。你不需要会设计，只需要把感觉**拆成下面这六类之一**，并说清「哪个区域、什么强度」。

### 1.1 「丑」的六类真实病因

| 你这样形容 | 真实病因 | 工程参数 | 怎么一眼验证 |
|---|---|---|---|
| 挤、闷、喘不过气 | **密度过高、缺留白**（density / whitespace） | `item_spacing`、`button_padding`、块间距、段落行高、面板内边距 | 截图里相邻元素的间隙小于等于元素自身高度的 1/4 |
| 乱、花、不知道看哪 | **层级失效**（hierarchy） | 灰阶阶梯、字阶、分区背景差 | 截成灰度图后，主次信息区分不出来 |
| 土、老气、像十年前的软件 | **视觉语言过时**（outdated visual language） | 默认控件外观、直角、重边框、高饱和色 | 色值等于框架出厂默认 |
| 暗沉、发糊、脏 | **对比度不足 / 无色相系统**（contrast / palette） | 前景-背景对比度、surface 分层、强调色 | 正文对比度低于 7:1 |
| 找不到重点、没有主次 | **缺强调与主操作**（emphasis / primary action） | 强调色只给一处、主次按钮分级 | 强调色出现次数大于等于 3 类元素 |
| 不像专业软件 | **缺状态反馈与细节**（affordance / polish） | hover / active / focus 四态、滚动条、空态、状态栏 | 鼠标划过界面，除了亮度微变没有任何反馈 |

> **判断顺序**：先看「层级」和「密度」（决定 80% 的观感），再看「色彩系统」，最后才是圆角阴影这些装饰。反着做是新手最常见的错 —— 给一个层级混乱的界面加圆角，只会更乱。

### 1.2 形容词 → 设计术语 → 具体参数（直接对照）

| 你说这个 | 我理解为 | 具体会改 |
|---|---|---|
| **清爽** | 低饱和 + 大留白 + 细边框 | 灰阶收敛到 6 档、间距阶梯上调一级、边框统一 1px |
| **高级 / 精致** | 灰阶系统 + 单一强调色 + 圆角一致 | 全套中性灰 + 一个 accent、圆角只有 3 个值、不加阴影 |
| **现代** | 8px 网格 + 圆角 6-8px + 扁平无拟物 | 所有间距是 4 的倍数，控件无渐变无内阴影 |
| **沉浸 / 专注** | chrome 退场 + 行宽约束 + 居中正文 | 侧栏/工具栏用更暗的表面，预览正文限宽 720px 居中 |
| **紧凑** | 密度上调（与「清爽」**相反取向**，必须二选一） | `item_spacing` 收紧、`button_padding` 减小 |
| **柔和** | 降对比 + 圆角加大 + 轻阴影 | 前景降一档、圆角 10px、浮层加漫射阴影 |
| **有呼吸感** | 段落与块间距 + 面板内边距 | 正文行高 1.7、块间距 16、面板 padding 16 |
| **别太花** | 强调色只保留一处 | accent 只用于选中态与主操作，语义色只用于错误提示 |
| **像 Obsidian / Linear / VS Code** | 指定参考坐标 | 见 §3.1，直接取该产品的具体特征 |

### 1.3 可以照抄的句式

```
<区域>的<元素>现在<现状>，我要它<目标形容词>，参考<产品>的<具体特征>。
```

现成八条，直接复制改：

1. 「侧边栏的选中项现在几乎看不出来，我要它一眼能认出，参考 Linear 左侧栏选中项那种带整行底色的做法。」
2. 「工具栏八个按钮一样重，我要它有主次，参考 VS Code 工具栏那种图标+分组+分隔线的做法。」
3. 「编辑区文字顶格贴着边、太闷，我要它有呼吸感，左右至少留 16px。」
4. 「预览区的代码块是一块直角灰块，太土，我要它现代一点，加 6px 圆角和一条细边框。」
5. 「三个栏的底色几乎一样，看不出分区，我要它分区明确，侧栏比内容区暗一档。」
6. 「选中文字那个蓝太扎眼、太老气，我要它柔和，换成低饱和的靛蓝。」
7. 「界面上除了鼠标划过微亮没有任何反馈，我要它专业，每个可点的东西都有悬停和按下两态。」
8. 「中文字挤在一起读着累，我要它舒服，正文行高加到 1.7、字间距略松。」

### 1.4 三种无效表达（会被追问，不如一开始就说清）

| 无效表达 | 为什么无效 | 换成什么 |
|---|---|---|
| 「高级一点」「有设计感」「大气」 | 形容词无指向，十个设计师能给十个答案 | 指定**参考产品 + 具体特征**：「参考 Linear 的侧栏，选中项整行底色 + 左侧 2px 强调条」 |
| 「颜色不好看」 | 没说哪一层不好看 | 说清是**surface / 前景 / 强调色**哪一层，以及太亮太暗太饱和 |
| 「都改一遍」 | 无法验收，且必然引入回归 | 指定**区域 + 优先级**：「先改外壳（侧栏/工具栏/页签），下次改编辑器」 |

### 1.5 二十个词，够用了

| 词 | 一句话解释 |
|---|---|
| surface | 一块背景面。层次靠「几档 surface」分，不靠边框 |
| chrome | 界面外壳（菜单、工具栏、侧栏），相对「内容区」而言 |
| accent | 强调色。全界面只用一种，给选中态和主操作 |
| hierarchy | 层级。谁重要谁次要，靠颜色/字号/间距表达 |
| density | 密度。同样空间放多少信息，紧 vs 松 |
| spacing scale | 间距阶梯。只允许用 4/8/12/16/24/32 这几个值 |
| token | 设计变量。颜色/间距/圆角的具名常量，唯一事实源 |
| corner radius | 圆角半径 |
| hairline | 1px 细线，用来分区（比阴影现代） |
| contrast ratio | 对比度，正文要 7:1 以上，次要文字 4.5:1 |
| gutter | 编辑器左侧行号列 |
| row height | 列表行高 |
| hover / active / focus | 悬停态 / 按下态 / 焦点态。每个可交互元素都要有 |
| disabled | 禁用态 |
| empty state | 空态。没数据时的引导画面，不是一行灰字 |
| status bar | 底部状态栏 |
| padding vs margin | 内边距（内）vs 外边距（外） |
| max width | 最大宽度。正文限宽是「专业感」的最大来源 |
| elevation | 层次感，用阴影或亮度表达「浮起」 |
| affordance | 可供性。看起来就能点的样子 |

---

## 2. 现状诊断（实测取证，不是感觉）

### 2.1 取证方法

- 在 Deepin / X11 下实跑 `target/release/latermd-app`，用 `xdotool search --name LaterMD` 取窗口 id 后 `import -window <id>` 精确截取应用窗口（不截桌面）。
- 对截图逐点采样像素值，与 egui 0.36.2 出厂默认常量对照（源码：`~/.cargo/registry/.../egui-0.36.2/src/style.rs`）。
- 对照代码：`crates/latermd-app/src/theme.rs`、`src/ui/*.rs`。

### 2.2 结论：整屏色值逐点等于 egui 出厂默认

| 区域 | 实测像素 | egui 默认 token | 结论 |
|---|---|---|---|
| 菜单栏 / 侧栏 / 工具栏 / 预览区底 | `rgb(27,27,27)` | `Visuals::dark().panel_fill = gray(27)` | 出厂值 |
| 编辑区底 | `rgb(10,10,10)` | `Visuals::dark().extreme_bg_color = gray(10)` | 出厂值 |
| 预览区代码块 | `rgb(64,64,64)` | `Visuals::dark().code_bg_color = gray(64)` | 出厂值 |
| 侧栏选中页签 | `rgb(0,92,128)` | `Selection::dark().bg_fill`（默认深蓝） | 出厂值 |
| 工具栏按钮面（悬停） | `rgb(63,63,63)` | `widgets.hovered.weak_bg_fill ≈ gray(60)` | 出厂值 |

**一句话定性：不是配色选错了，是从来没有做过视觉设计。** `theme.rs` 目前只做 `ctx.set_theme(Light|Dark)`，把「主题」等同成了「明暗二选一」，没有任何 token 覆盖。所以界面长什么样，完全由 egui 作者 Emilk 的默认审美决定。

这个结论决定了改造路径：**不是「调几个颜色」，而是「建立 token 层」**（§4）。

### 2.3 十一条病征

严重度：🔴 影响可用性 / 🟠 明显拉低质感 / 🟡 细节缺失

| # | 病征 | 证据 | 病因（代码位置） | severity |
|---|---|---|---|---|
| 1 | 三栏底色几乎相同，分区看不出来 | 侧栏与预览区都是 `gray(27)`，只有 1px 暗线 | `layout.rs` 三处 `Panel` 未设 `frame.fill` | 🟠 |
| 2 | 编辑区像贴上去的黑洞 | 编辑区 `gray(10)` 与外壳 `gray(27)` 断层，无过渡无边框 | `editor.rs` 裸 `TextEdit`，背景走 `extreme_bg_color` | 🔴 |
| 3 | 选中项几乎不可辨 | 页签选中 `rgb(0,92,128)` 深蓝，在 `gray(27)` 上对比度极低 | `sidebar.rs::tab_bar` 用 `selectable_label` | 🔴 |
| 4 | 没有强调色 | 全屏找不到一处统一的强调色 | 未设 `Visuals::selection` / `hyperlink_color` | 🟠 |
| 5 | 工具栏是八个等重文字按钮，还会换行 | 800px 宽下「导出 HTML / 设置」掉到第二行 | `toolbar.rs` 8 个 `Button` 平铺，无图标无分组 | 🟠 |
| 6 | 编辑器文字顶格贴边 | 源码紧贴面板左边缘与上边缘 | `editor.rs` 无内边距包裹、无 gutter | 🟠 |
| 7 | 无行号（gutter） | 编辑区左侧直接是文字 | `editor.rs` 未绘制行号列 | 🟡 |
| 8 | 预览代码块是直角灰块 | `gray(64)` 直角矩形，无边框 | `MarkdownStyle` 全默认 | 🟡 |
| 9 | 中文行距紧，长段读着累 | 中文与西文混排，行高按西文默认 | `MarkdownStyle` 默认 + `extra_text_line_spacing = 0` | 🟠 |
| 10 | 预览区被挤到 ~300px，表格折行 | 800px 窗口下 240+500 分掉后只剩 300 | `layout.rs` 固定 `default_size`，无最小宽度保障 | 🔴 |
| 11 | 无状态栏、无空态 | 底部直接是窗口边；无根目录时只有一行弱化文字 | 未实现 | 🟡 |

### 2.4 用户截图（Windows 侧）交叉验证

同一批默认值在 Windows 上复现（`panel_fill = gray(27)`、编辑区 `gray(10)`、代码块 `gray(64)`），说明这是**跨平台一致的默认态**，不是平台渲染差异。同时也确认了 M0 的 CJK 字体注入确实生效（无方块字）。

---

## 3. 视觉定稿

### 3.1 参考坐标系（不是抄，是定锚）

| 参考 | 取它的什么 | 不取什么 |
|---|---|---|
| **Obsidian** | 三栏布局的呼吸感、正文限宽居中、大纲层级 | 它的插件生态视觉杂乱 |
| **Linear** | 克制的灰阶、单一 accent、选中态用整行底色而非高饱和、极细边框 | 它的密集列表（写作工具不需要那么密） |
| **VS Code** | 侧栏比内容区暗一档的分区逻辑、编辑器 gutter、工具栏图标+分组 | 它的高信息密度与多面板堆叠 |
| **Typora** | 预览正文的排版质量：标题字阶、段间距、代码块留白 | 它的无 chrome 单栏（我们要保留三栏） |

**产品气质命名：克制的纸感编辑器（Quiet Paper）。** 一句话：内容像纸一样浮在最上层，外壳安静地退到后面，全界面只有一个强调色。

### 3.2 六条设计原则

1. **内容优先，chrome 退场。** 内容区（编辑/预览）比外壳（侧栏/工具栏/菜单栏）**亮一档**。这是「沉浸感」的全部秘密 —— 文字所在的表面必须是画面里最亮的。
2. **单一强调色。** accent 只出现在三处：当前选中项、主操作按钮、可点链接。超过三处就失效。
3. **灰阶分层代替边框。** 分区优先用背景差，边框只在确实需要分隔时用 1px hairline，绝不出现 2px 以上的框。
4. **8px 节奏。** 所有间距取自 4/8/12/16/24/32，不出现 7px、13px 这类随手值。对齐产生秩序感。
5. **一致圆角与线宽。** 圆角只有 3 个值（4/6/10），线宽只有 1。不做「这个控件圆一点那个方一点」。
6. **每个可交互元素都有四态。** idle / hover / active / disabled，肉眼可辨。这是「专业感」的主要来源，成本最低收益最高。

### 3.3 做 / 不做

| 做 | 不做 |
|---|---|
| 用背景层次分区 | 用粗边框、阴影分割 |
| 一个 accent 贯穿 | 每个功能一个颜色 |
| 细线、小圆角、低饱和 | 渐变色、拟物高光、大圆角气泡 |
| 图标 + 文字（工具栏） | 纯文字按钮平铺 |
| 正文限宽居中 | 预览文字横跨整个屏幕宽度 |
| 空态给引导动作 | 空态只放一行灰字 |
| 状态栏承载信息 | 信息塞进工具栏挤到换行 |

---

## 4. Design Tokens（可直接抄进 `theme.rs`）

### 4.1 色彩

**暗色（`ThemeMode::Dark`，当前默认）**

| token | 值 | 用途 / 映射 |
|---|---|---|
| `surface_sunken` | `#141517` | 外壳：菜单栏、侧栏、工具栏、状态栏 |
| `surface` | `#1A1B1E` | 内容区：编辑区、预览区 |
| `surface_raised` | `#232529` | 浮层：下拉菜单、对话框、hover 行 |
| `surface_hover` | `#25272C` | 列表行悬停 |
| `selection_bg` | `#2A3350` | 选中行/选中页签底色（低饱和靛蓝，非高饱和） |
| `border` | `#2A2D33` | 1px 分区线 |
| `border_strong` | `#3A3E45` | 输入框等需要明确边界的控件 |
| `text_primary` | `#E6E7EA` | 正文、标题 |
| `text_secondary` | `#9A9DA5` | 次要信息、行号、快捷键提示 |
| `text_muted` | `#6B6F78` | 禁用态、占位文本 |
| `accent` | `#7C8CFF` | 强调：选中、焦点环、链接、主操作 |
| `accent_hover` | `#8F9DFF` | 强调色悬停 |
| `code_bg` | `#232529` | 代码块背景 |
| `error` / `warn` / `ok` | `#F2555A` / `#E8A33D` / `#4CC38A` | 语义色，只用于提示行 |

**亮色（`ThemeMode::Light`）**

| token | 值 | 说明 |
|---|---|---|
| `surface_sunken` | `#F2F3F5` | 外壳 |
| `surface` | `#FFFFFF` | 内容区 |
| `surface_raised` | `#FFFFFF` | 浮层（配 1px `border` + 轻阴影） |
| `surface_hover` | `#F0F1F4` | 列表行悬停 |
| `selection_bg` | `#E7EAFB` | 选中行底色 |
| `border` | `#E3E5E9` | 分区线 |
| `border_strong` | `#CFD3DA` | 控件边界 |
| `text_primary` | `#1B1D21` | 正文 |
| `text_secondary` | `#5C6069` | 次要信息 |
| `text_muted` | `#8A8F99` | 禁用态 |
| `accent` | `#4C5FD5` | 强调（白底上对比度 5.4:1，可作正文链接） |
| `code_bg` | `#F6F7F9` | 代码块背景 |

**对比度自查（已核对 WCAG AA/AAA）**：暗色正文 13.8:1、次要 6.3:1；亮色正文 16.8:1、次要 6.4:1；accent 暗色 5.8:1、亮色 5.4:1。全部 ≥ 4.5:1，正文两档均 ≥ 7:1。

> **accent 备选**（若靛蓝不合口味，换这一个值即可全站生效，其余 token 不动）：
> 青绿 `#2E9E83`（更冷静，偏工程感）｜ 琥珀 `#C9852A`（更温暖，偏文学感）。
> 暗色模式各加亮一档：`#3FBFA0` / `#E0A34D`。

### 4.2 间距 / 圆角 / 线宽

| token | 值 | 用途 |
|---|---|---|
| `space_xs` / `space_sm` / `space_md` / `space_lg` / `space_xl` / `space_2xl` | 4 / 8 / 12 / 16 / 24 / 32 | 全部间距只能取这六个值 |
| 面板内边距 | 16 水平 / 12 垂直 | 侧栏、预览区 |
| 编辑区内边距 | 16 水平 / 14 垂直 | 沉浸感的关键，不能是 0 |
| 列表行高 | 26 | 文件树、大纲 |
| 半径 `radius_sm` / `radius_md` / `radius_lg` | 4 / 6 / 10 | sm 小控件、md 按钮与代码块、lg 浮层 |
| 线宽 | 1.0 | 唯一值 |

### 4.3 字阶与行高

| 用途 | 字号 | 行高 | 说明 |
|---|---|---|---|
| UI 正文 | 13 | 1.4 | 工具栏、页签、列表 |
| UI 次要 | 12 | 1.4 | 快捷键提示、行号、状态栏 |
| UI 标题 | 14 | 1.4 | 面板标题 |
| **预览正文** | 16 | **1.7** | 中文行高必须比西文大；这是「读着累」的根治 |
| 预览代码 | 13.5 | 1.6 | 等宽 |
| 编辑区源码 | 13.5 | 1.6 | 与预览代码同值，切换不跳 |

`extra_text_line_spacing` 由行高差反推，不写死：`行高 × 字号 - 字体行高`。

### 4.4 动效

| 项 | 值 | 理由 |
|---|---|---|
| `Style::animation_time` | 0.08 | egui 默认 0.083s 已经合适，保持 |
| 主题切换 | 0 过渡 | 立即模式，同帧重投影，不做淡入（淡入会闪） |
| 面板折叠/展开 | 用 egui 内建 | 不自己写动画 |

> 「零延迟」是编辑器的手感底线。任何超过 120ms 的过渡都会被感知为「卡」。

### 4.5 token → egui 0.36.2 字段映射（落地对照，字段名已核对源码）

| token | egui 落点 |
|---|---|
| `surface_sunken` | `Visuals::panel_fill`；三个外壳 `Panel` 的 `frame.fill` |
| `surface` | `Visuals::text_edit_bg_color = Some(surface)`（**关键**：覆盖 `extreme_bg_color`）；`CentralPanel` 的 `frame.fill` |
| `surface_raised` | `Visuals::window_fill` |
| `border` | `Visuals::window_stroke`、`widgets.noninteractive.bg_stroke` |
| `text_primary` | `Visuals::override_text_color` |
| `text_secondary` | `widgets.noninteractive.fg_stroke.color` |
| `accent` | `Visuals::selection.bg_fill`、`hyperlink_color`、`widgets.active.bg_fill`、`text_cursor.stroke` |
| `code_bg` | `Visuals::code_bg_color` |
| 圆角 | `Visuals::window_corner_radius` / `menu_corner_radius` / `widgets.*.corner_radius` |
| 间距 | `Style::spacing.item_spacing` / `button_padding` / `interact_size` / `menu_margin` / `extra_text_line_spacing` |
| 正文（预览） | `MarkdownStyle` 装进 `ThemeSettings::overrides`（`theme.rs:71` 已预留该字段，批次 A 一直为 `None`） |
| 代码高亮 | `MarkdownStyle::code_theme`（不传则自动跟随明暗；批次 B 可指定 syntect 主题名） |

**双主题写入方式**：用 `Context::style_mut_of(Theme::Light|Dark, ...)` 或 `set_visuals_of(theme, visuals)`，**不要**用 `set_visuals`（那会破坏 `theme.rs::apply` 现有的「`ctx.theme()` 切换 + 幂等 staleness 检查」契约）。两份 token 表各自投影，`apply()` 的现有逻辑一行不用改。

---

## 5. 组件级规范

每件给「现状 → 目标 → 落点 → 验收」。

### 5.1 菜单栏
- **现状**：`Panel::top` + egui 默认菜单，浅/深跟随主题，无样式。
- **目标**：背景 `surface_sunken`，高度压到 30px，菜单项悬停用 `surface_hover`，快捷键提示用 `text_secondary`。
- **落点**：`ui/menubar.rs` + `layout.rs` 的 `Panel::top` frame。
- **验收**：菜单栏与工具栏之间有 1px `border` 分隔，不是一个色块连着另一个色块。

### 5.2 工具栏（重点）
- **现状**：8 个等重文字按钮平铺，800px 宽下换行。
- **目标**：
  - **分组**：文件组（新建/打开/保存/另存为）｜导出组｜右侧设置。组间 1px 竖分隔。
  - **图标 + 文字**：图标用 egui 内建 emoji 字符（不引图标库），文字保留可发现性。
  - **主次**：「保存」是唯一的主操作，用 accent 描边；其余为次级，`weak_bg_fill` 透明、悬停才出底色。
  - **不换行**：面板窄到放不下时，把**文字**收掉只留图标（而不是换行）。快捷键提示从按钮内移到 hover tooltip。
- **落点**：`ui/toolbar.rs`、`command.rs` 补图标字段。
- **验收**：窗口宽 760px 时不换行；8 个按钮中主操作一眼可辨。

### 5.3 侧栏页签（重点）
- **现状**：`selectable_label`，选中是 egui 默认深蓝，几乎不可辨。
- **目标**：胶囊形（`radius_sm`）分段控件。选中 = `selection_bg` 底 + `accent` 文字 + 500 字重；未选中 = 透明底 + `text_secondary`；整条页签栏下方 1px `border`。
- **落点**：`ui/sidebar.rs::tab_bar`。
- **验收**：灰度截图下选中页签仍可辨（即不依赖色相区分）。

### 5.4 文件树行 / 大纲行
- **现状**：`selectable_label`；当前文件用默认蓝高亮；无行高控制。
- **目标**：行高 26；整行可点（不是只有文字可点）；悬停 `surface_hover`；当前项 `selection_bg` 底 + 左侧 2px `accent` 竖条；目录/文件用不同图标区分；缩进 14px/级（沿用 `OUTLINE_INDENT`）。
- **落点**：`ui/sidebar.rs`、`filetree.rs`（只加图标枚举，不改状态机）。
- **验收**：点击行内空白也能打开文件；当前项有左侧强调条。

### 5.5 编辑器（重点）
- **现状**：裸 `TextEdit::multiline`，无内边距、无 gutter，背景 `gray(10)` 成黑洞。
- **目标**：
  - 背景改 `surface`（与预览区同色），与外壳形成「内容层更亮」的正确关系。
  - 外裹 `Frame::inner_margin(16, 14)`，文字不再贴边。
  - 左侧加 gutter 行号列（宽 40，右对齐，`text_secondary`，当前行用 `text_primary`）。行号由缓冲行数 × 行高推出，**自绘**，不用 TextEdit 内建能力（它没有）。
  - 行高 1.6（`extra_text_line_spacing` 反推）。
- **落点**：`ui/editor.rs`。
- **验收**：编辑区与预览区底色一致、可区分于侧栏；左侧有行号且随滚动对齐。
- **风险提示**：gutter 与 `TextEdit` 的滚动同步是这一节唯一的难点 —— `TextEdit` 自己管内部滚动，行号列需读 `TextEditOutput` 的 `galley` 与 `galley_pos` 反推，**不要**另建 ScrollArea（会双滚动条）。

### 5.6 预览区（重点）
- **现状**：全宽铺满，无内边距；`MarkdownStyle` 全默认。
- **目标**：
  - 内边距 16；正文**限宽 720px 居中**（`ui.set_max_width`），超出部分留白 —— 这是「专业感」最廉价的来源。
  - `MarkdownStyle` 覆盖：`block_spacing = 16`、标题字阶收紧、代码块 `radius_md` + `border`、表格线用 `border` 色、行内代码用 `code_bg` + 小内边距。
  - 中文行高 1.7（靠 `text_styles` 映射）。
- **落点**：`ui/preview.rs` + `theme.rs` 的 `overrides`（**激活那个一直为 `None` 的字段**）。
- **验收**：宽屏下正文不横跨全屏；代码块有圆角与细边框。

### 5.7 分隔条 / 滚动条
- **现状**：egui 默认（滚动条 `bar_width` 默认值，分隔条无视觉）。
- **目标**：滚动条 `bar_width = 8`、`handle_min_length = 24`、圆角、`bar_inner_margin = 2`、非悬停时低不透明度；面板分隔条悬停时显 `accent` 色 1px。
- **落点**：`Style::spacing.scroll`、三个 `Panel` 的 resizable 分隔条。
- **验收**：不滚动时滚动条几乎不可见，悬停才显形。

### 5.8 状态栏（新增）
- **现状**：无。
- **目标**：底部 22px 高 `surface_sunken` 条，上边 1px `border`。左：当前文件路径（`text_secondary`，超长中间省略）；中：光标行列；右：字数 / 编码 / 行尾符（CRLF/LF）。文字 12px。
- **落点**：`layout.rs` 末尾加 `Panel::bottom`（**注意顺序**：必须在 `CentralPanel` 之前）。
- **验收**：窗口底部常驻，切换文件即时更新。

### 5.9 空态（新增）
- **现状**：无根目录只显示一行弱化文字。
- **目标**：居中的引导块 —— 图标 + 一句说明 + 一个**主操作按钮**（accent）。三处空态：无根目录（选目录）、空目录、文档无标题（大纲页）。
- **落点**：`ui/sidebar.rs::placeholder` 升级为 `empty_state(ui, icon, text, action)`。
- **验收**：空态里能直接完成下一步操作，不用去找别处的按钮。

### 5.10 提示行（文件操作失败）
- **现状**：`ui.colored_label(error_fg_color, notice)` + 「知道了」小按钮，挤在工具栏里导致换行。
- **目标**：改为工具栏下方独立一条 24px 高提示条，`error` 色左边框 2px + 淡背景，右侧 × 关闭。不挤压工具栏。
- **落点**：`ui/toolbar.rs::ui` 的第二段。
- **验收**：出现提示时工具栏不换行。

---

## 6. 落地批次

### 6.1 P0.5 视觉基线（**建议提前做，不等到 P2.5**）

| 项 | 内容 | 工作量 |
|---|---|---|
| ① | `theme.rs` 建 token 层：两套 `ThemeTokens` 常量 + `style_mut_of(Theme, ...)` 投影 | 0.5 天 |
| ② | 外壳三件：侧栏页签胶囊化、工具栏图标+分组、三栏背景分层（含状态栏） | 0.5 天 |
| ③ | 内容两件：编辑区内边距 + 背景归位、预览限宽 720 + `MarkdownStyle` 覆盖 | 0.5 天 |

**为什么建议提前**（这是与 roadmap 唯一的分歧点，请拍板）：

1. roadmap 自己写了「Live Preview 必须在**视觉终态**上开发，否则聚焦/半隐藏样式按旧视觉调一遍、换肤后再返工一遍」。同理，**P1 的全文搜索、AI 流式面板、P2 的 Git 面板**都在 P2.5 之前，它们全都要在旧视觉上先写一遍。
2. token 化的成本与「界面元素数量」成正比。现在只有 7 个 UI 文件、1700 行 UI 代码；P1+P2 之后会翻倍甚至翻三倍。**越晚做越贵。**
3. 这 1.5 天不产出新功能，属于纯品质投入 —— 若你认为 P0 出包优先于观感，可以只做其中的 ①②（1 天），把 ③ 留到 P2.5。

### 6.2 P2.5 皮肤系统（roadmap 原计划，不缩水）

在 P0.5 的 token 层之上补：三态（亮/暗/跟随系统）、自定义皮肤文件（`ThemeTokens` serde → RON 至配置目录 `themes/`）、gutter 与滚动条的进一步打磨。**因为 token 层已就位，这部分从 1.5-2 周可以压到 1 周。**

### 6.3 明确不做

- 图标库（`egui` 内建字符不够用再说，先不引依赖）
- 动画框架、页面转场
- 面板布局自定义 / 拖拽重排
- 皮肤市场、CSS 引擎、每控件样式树

---

## 7. 验收标准

**可测量（机器可验）**

| 项 | 标准 |
|---|---|
| token 唯一来源 | `crates/latermd-app/src/` 下无裸 `Color32::from_rgb`（测试/字体除外） |
| 间距合规 | 所有 `add_space` 参数 ∈ {4,8,12,16,24,32} |
| 对比度 | 正文 ≥ 7:1，次要文字 ≥ 4.5:1（两套皮肤各测一次） |
| 内容层更亮 | `surface` 亮度 > `surface_sunken` 亮度（两套皮肤均成立） |
| 不换行 | 窗口宽 760px 时工具栏单行 |

**主观（人眼对照）**

- 三平台各截一张图，与 §3.1 参考产品的气质对照：像「安静的纸」，不像「框架 demo」。
- 截成灰度图后，三栏分区与主次仍可辨（不依赖色相）。
- 鼠标划过每个可交互元素，都有明确反馈。

**回归底线**

- `cargo fmt --all --check` / 三轮 clippy / test / doc 全绿（合入前本地跑完，CI 只在 main 跑）。
- 主题切换仍然同帧生效、无闪变、重启保持（现有 `theme.rs` 测试不得修改语义）。
- 不修改 vendor 任何文件；不改动其他三个 crate。

---

## 8. 与既有文档的关系

| 文档 | 关系 |
|---|---|
| [AGENTS.md](../AGENTS.md) §7 | P2.5「界面打磨」的范围来自此处，本文档不扩范围 |
| [docs/roadmap.md](roadmap.md)「专题：界面美化与皮肤系统」 | **上位文档**。批次划分、周期、风险归它；本文档是其批次 B 的视觉细化 |
| [docs/adr-005-layout-and-sidebar.md](adr-005-layout-and-sidebar.md) | 布局与 `logic`/`ui` 二分契约由它定。视觉改动全部落在 `ui` 侧，**不得**触及归约逻辑 |
| [docs/p0-acceptance.md](p0-acceptance.md) | P0 验收不含视觉项；若采纳 §6.1，在 P0 后追加一条「视觉基线」验收 |
| [docs/m0-report.md](m0-report.md) | 中文字体方案（`fonts.rs` 候选表）由它定，本文档只依赖其结论 |

---

## 修订记录

| 日期 | 变更 |
|---|---|
| 2026-09-25 | 初版。含需求表达词库（§1）、实测诊断（§2）、token 定稿（§4）、组件规范（§5）、P0.5 提前建议（§6.1） |
