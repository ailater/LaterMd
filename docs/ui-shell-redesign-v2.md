# 外壳改版 v2：信息架构优先的第二次收口

日期：2026-10-08
状态：**S1 已落地**（`feature/ui-shell-s1`，commit `c9e4c87`）；S2/S3 待排期
关联：[ui-shell-redesign.md](ui-shell-redesign.md)（第一次收口，2026-09-26）、
[ui-polish.md](ui-polish.md)、[ui-modernization.md](ui-modernization.md)（第三方 UI 库裁决）、
[decisions-pending.md](decisions-pending.md)

> 本文是**第二次收口**。第一次（`ui-shell-redesign.md`）解决的是「三栏怎么摆」，
> 留下的执行结果经真机使用暴露了一个新问题：**每条栏都按「功能归属」设计，
> 没有按「信息密度」设计**。结果是 chrome 吃掉了主工作区。
>
> 与第一次的关系：**不推翻**。三分栏、菜单栏负责全部命令、图标自绘、
> 状态栏收成一行 —— 这些既定决策全部保留。v2 只做**减法与重排**。

---

## 1. 诊断：七个症状与它们的根因

按真机截图取证，每条都落到具体代码位置而非主观感受。

| # | 症状 | 根因 | 常驻代价 |
|---|---|---|---|
| A | 顶部三条横栏 | `layout.rs` 两个 `Panel::top`（标题栏 36 + 菜单栏 ~24）+ CentralPanel 内的 `format_bar` 30 | ~90px |
| B | 左栏顶部六个无标签图标 | `sidebar.rs top_actions` = `Command::FILE`(4) + 导出 2，纯图标无文字 | 34px + 认不出 |
| C | 视图导航竖排五行 | `SidebarTab::ALL` 五项 × `NAV_ROW_H` 26 | **130px** |
| D | 格式条 17 按钮等权重平铺 | `FormatGroup` 四段结构逻辑存在、视觉上无分组；`B I S` 用字母而其余用自绘线段 | 30px + 扫读成本 |
| E | minimap 像「第三栏」 | `minimap.rs` 右缘窄条独占 96px，与预览栏边界混同 | 96px |
| F | 状态栏信息全挤左 | `layout.rs status_bar` 单一 `horizontal_wrapped` 顺排 | 右侧数百像素空白 |
| G | 浅色下侧栏与编辑区近乎同色 | `theme.rs` 的 `panel_fill` / `content_fill` 差 <3% | 层级感弱 |

**最刺眼的是 C**：左栏顶部图标条（34）+ 视图导航（130）= 164px 常驻，
而它们下面才是真正的主工作区（文件树）。**侧栏顶部比侧栏内容还忙。**

---

## 2. S1-1 视图导航：竖排五行 → 单行图标 tab

**已落地**（`sidebar.rs view_nav` / `paint_nav_tab` / `nav_tab_center`）

```
改前（130px）              改后（26px）
┌────────────┐            ┌────────────┐
│ 📁 文件      │ ← 选中     │ ▣▣▣▣▣      │ ← 26px，下缘横条表选中
│ 🔍 搜索      │            ├────────────┤
│ ☰ 大纲      │            │ 文件树     │
│ ⑂ Git       │            │ （+104px）  │
│ 🔗 链接      │            │            │
└────────────┘            └────────────┘
```

| 决策 | 选择 | 理由 |
|---|---|---|
| 排列 | 横排五枚 26×26 tab | 5×26+4×2 = 138px ≤ `SIDEBAR_MIN_W` 180，窄栏也放得下 |
| 选中态 | 圆角底 + **下缘 2px 横条** | 横排时左侧竖条会指向相邻 tab（"所属行"已读不出来）；横条与 `ui::tabs` 页签下划线同构 |
| 标签 | 只给 tooltip，不给常驻文字 | 常驻文字在 180px 下限会换行（`top_actions` 已有前例） |
| 命中区 | 整枚 26×26 | 不是仅图标 |
| `NAV_ROW_H` | 标废弃但**保留** | 旧 commit 的测试按名字引用它，删常量破坏 `git bisect` |
| `icon_label_row` | 标 `#[allow(dead_code)]` 保留 | 同上；它记录了「整行选中态」的画法 |

**测试**：`nav_tab_center_x_follows_tab_order`（顺序 helper 改 x 轴）、
新增 `five_nav_tabs_fit_the_narrowest_sidebar`（138 ≤ 180，改版前提）、
`clicking_nav_tab_switches_view`（点击路径 + 落点在段内的 sanity 断言）。

### 2.1 一条测试纪律：断言上界不许复用绘制 token

`three_bands_fill_the_panel_top_down` 断言 nav 段高度 ≤ 40px。这个 40
**刻意硬编码，不读 `NAV_TAB_H`**。

原因：段高由 `set_min_height(top + NAV_TAB_H)` 报告，与绘制**同源**。
若上界也读 token，把 token 调大（哪怕是误改）会让绘制与断言一起变大 →
断言自我满足。红绿验证实测：

| 断言写法 | 模拟回退（`NAV_TAB_H` 改 130） | 结论 |
|---|---|---|
| 上界 = `NAV_TAB_H + SEP + 1` | **绿**（133px ≤ 134px） | ❌ 失效 |
| 上界 = 硬编码 `40.0` | **红**（133px > 40px） | ✅ 有效 |

**通则**：断言「不超过某量」时，上界必须独立于被断言的实现。

---

## 3. S1-2 状态栏：单一顺排 → 三段分区

**已落地**（`layout.rs status_bar`）

```
改前                          改后
文件名 行21:1 · 248字 ·        文件名 行21:1     248字        浅色 · Mock · MCP: 8731
浅色 · Mock · MCP: 8731       └─ 左 ──────┘  └ 中 ┘  └──────── 右 ────────┘
└──── 全部挤在左端，右端空 ────┘
```

| 段 | 内容 | 理由 |
|---|---|---|
| 左 | 文件名 · 行:列 | 最高频，与文档强绑定 |
| 中 | 字数 | 写作进度感，与左右都无关联 |
| 右 | 主题 · AI provider · MCP | 全是**全局服务状态**，彼此相关，应聚在右端一眼扫完 |

**核心收益不是「好看」，是告警位置**：`MCP: 启动失败` 是状态栏里唯一
必须被盯到的动态项，改版前它偏在最右、要横跨整屏才看到；改版后恒在
右下角，与视线停留点一致。

| 决策 | 选择 | 理由 |
|---|---|---|
| 右段布局 | `Layout::right_to_left` | 零参数、自动贴边、窗口拉伸无需同步改数字 |
| 段内顺序 | push 顺序即从右到左 | MCP 第一个 push → 画在最右 |
| 窄窗口 | 中段整段不画（`STATUSBAR_MIN_W=240`） | 字数是三者里唯一丢了不影响操作的；**绝不让 MCP 告警被挤出可视区** |
| `separator()` | 删除 | 三段本身已表达分组，`·` 分隔符反而暗示「同组」 |

**不用 `layout_to_min_x` 的理由**：它要求调用方自己算百分比 x，三段各写
一个 magic number；「左中顺排 + 右段 `right_to_left`」是零参数写法。

---

## 4. S2/S3 待排期（本批不做）

| 档 | 项 | 收益 | 风险 |
|---|---|---|---|
| S2-1 | 标签条上移并入标题栏；搜索框 + 源码/Live 分段控件放标题栏右端 | 省 28px | **中高** —— 无边框模式标题栏是唯一窗口拖动区，加控件要避 `edge_resize_zones`；Zen 的四条退出路径全挂这条上 |
| S2-2 | 格式条 17 按钮 → 3 组 + 溢出菜单 | 省 30px 常驻 | 中 —— `format_bar.rs` 有「每动作必有命令」「图标互不重复」两条守门测试要重跑 |
| S2-3 | `panel_fill` 与 `content_fill` 拉大明度差（浅色下 −4%） | 三栏层级立刻清晰 | 低 —— 影响所有 `Frame::fill(panel_fill)` 处，需截图回归 |
| S3-1 | minimap 改浮在编辑器右下角（半透明、`Ctrl+M` 折叠） | 消除「第三栏」错觉 | 低 —— `minimap.rs` 的让位断言要跟着改 |

---

## 5. 明确不做

| 方案 | 否决理由 |
|---|---|
| 引第三方 UI 库（egui-thematic / Armas） | `ui-modernization.md` 已裁决：锁 egui 0.33 与 0.36.2 **类型不通**（`Visuals` 是 struct 非 trait，跨版本报 E0308），且下载量个位数、与自有 `theme.rs` + `themes/*.ron` 重复 |
| 给工具栏加常驻文字标签 | 侧栏拖到 180px 下限会立刻换行，反而更乱。这是 S1-1 选「tooltip」而非「常驻文字」的原因 |
| 动 `Command::ALL` 44 条的菜单覆盖 | `menubar.rs` 的 `toggle_checked` 勾选态刚落地（#129），改版不与之抢 |
| 一次改完 S1+S2+S3 | S2-1 碰窗口拖动区与 Zen 出口，混批会让 S1 的低风险特性无法独立回滚 |

---

## 6. 门禁（S1 head `c9e4c87` 实跑）

| 项 | 结果 |
|---|---|
| `cargo fmt --all --check` | ✅ |
| `cargo clippy --workspace --all-targets -- -D warnings` ×3（default / `--no-default-features` / `--all-features`） | ✅ |
| `cargo test --workspace --all-features` | ✅ **1381 passed / 0 failed**（main 为 1380，+1 新测试） |
| `cargo doc --no-deps --all-features` | ✅ 零 warning |

**真机目视留人工**（本机 Linux 可跑 X11，但需要人工判断观感）：
五枚 tab 的选中横条在 180px 窄栏下是否够清晰、状态栏右段在 1080p 下的
三段留白是否匀称、暗色主题下 hover 底色是否可辨。
