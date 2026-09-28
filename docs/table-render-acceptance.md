# 表格渲染收口:明/暗两套像素采样验收(2026-09-29)

> 验收对象:#30 表格渲染三模块——T1 边框(25d131f)、T2 vendored 底色能力(6191beb,①上游可合)、T3 本棒接线(出厂默认与九套预设开 `header_fill`/`zebra_fill` + "bold" 字族注册)。
> 方法:照抄 [m5-acceptance.md](m5-acceptance.md) —— `cargo build -p latermd-app` → `DISPLAY=:0` 启动 → `import -window <id>` 截图 → python3+PIL 全图逐行采样。
> 截图存 `/tmp/t30-dark.png`(深色主窗)、`/tmp/t30-light.png`(浅色主窗)、`/tmp/t30-light-zen.png`(禅定)、`/tmp/t30-light-midscroll.png`(宽表横向滚动中段);md 只记数字与结论。
> 窗口 900×600,干净配置目录(`XDG_CONFIG_HOME=/tmp/t30cfg`,九套预设重新铺盘含新字段),深色 `mode=dark`、浅色 `mode=light` 各启动一次。

## 0. 环境备注(影响验收操作方式的三件事,非应用缺陷)

1. **锁屏再现**:Deepin `dde-lock` 又盖住全屏(`_NET_ACTIVE_WINDOW` 为 0x0,合成键零到达),与 m5 §0.1 同款;同用户 `kill` 后恢复,处置照抄前案。
2. **IME 吃合成按键**:`xdotool type` 打 ASCII 被 fcitx 截获拼成中文(「Name」→「N阿么」)。入稿改走**剪贴板粘贴**(`xclip -i` + Ctrl+V),粘贴字节直通不经 IME;粘贴后 `ctrl+a/ctrl+c` 回读缓冲逐行核对(11 行,与样例文档一致)。
3. **`--window` 合成事件不达**:`xdotool key --window` 的 XSendEvent 进不了 winit;须先 `windowactivate` + 真实鼠标点击聚焦,再用无 `--window` 的 XTEST 事件。

## 1. 样例文档(两表:窄表验主断言,宽表验横向滚动)

```markdown
| Name | Value | Note |
|---|---|---|
| alpha | 1 | first row |
| beta | 2 | second row |
| gamma | 3 | third row |
| delta | 4 | fourth row |

| Col A | Col B | Col C | Col D | Col E | Col F |
|---|---|---|---|---|---|
| long-field-one | long-field-two | long-field-three | long-field-four | long-field-five | long-field-six |
| more-a | more-b | more-c | more-d | more-e | more-f |
```

预览栏实测几何(两套主题同构同坐标,m5 口径):窄表 x[489,700](3 列,竖线 x=489/553/609/700),宽表溢出预览栏(x[489,896] 可见,右缘裁切)。行带(两主题逐像素一致):

| y 区间 | 归属 |
|---|---|
| 85 | 窄表顶边框线 |
| 86–111 | 窄表表头行(faint 底) |
| 112 | 表头下分隔线 |
| 113–139 / 141–167 / 169–195 / 197–223 | 数据行 alpha/beta/gamma/delta |
| 224 | 窄表底边框线 |
| 240–323 | 宽表(顶线 240、表头 241–266、斑马行 268–294、底线 323) |

## 2. 深色断言(mode=dark,`/tmp/t30-dark.png`)

取色源 `theme.rs::shell_tokens`:正文底 content `#292A2D`、faint `#2A2B2E`、border `#3C4043`(`noninteractive.bg_stroke`)。

| 断言 | 证据(实测数字) | 结论 |
|---|---|---|
| ① 表头行底色 ≠ 正文底色 | 表头带 y88–109 逐行:faint 206–208px vs content 203px(表右侧底),两色逐像素可分(`#2A2B2E` vs `#292A2D`,PNG 无损精确) | PASS |
| ② 数据区隔行色差 | alpha(y126) faint=151;beta(y154) faint=**0**、content=347;gamma(y182) faint=144;delta(y210) faint=**0**——偶数行斑马、奇数行正文底,交替成立 | PASS |
| ③ 外框线可检出 | 顶 y85 border 200px、底 y224 border 200px、表头分隔 y112 border 211px;竖线 x=489/553/609/700;宽表顶/底 389px | PASS |

## 3. 浅色断言(mode=light,`/tmp/t30-light.png`)

token:content `#FFFFFF`、faint `#FAFBFC`、border `#E5E6EB`。行带坐标与深色**逐带一致**(§1 表),同脚本采样:

| 断言 | 证据(实测数字) | 结论 |
|---|---|---|
| ① 表头行底色 | 表头带 y88–109:faint 206–208px vs content 203px | PASS |
| ② 数据区隔行色差 | alpha faint=151 / beta faint=0 / gamma faint=144 / delta faint=0(与深色同构) | PASS |
| ③ 外框线 | 顶/底 200px、分隔 211px、竖线 x=489/553/609/700、宽表顶/底 389px | PASS |

## 4. 横向滚动 / 禅定场景(#30 T3 第 4 点)

- **溢出裁切(两主题,主窗态)**:宽表内容 ~650px > 预览栏 ~415px。右缘裁切处实测:深色 y250 x890–892 为阴影渐变(37,37,41)/(54,57,60),x893 起即正文底——底色**不出血**到视口外;外框线沿可见矩形内画(StrokeKind::Inside)。
- **滚动中段(浅色,`/tmp/t30-light-midscroll.png`)**:横向滚动条悬停显形(y324,thumb 最左 = scroll 0),拖到中段后:表头底 x[490,875]、顶/底框线 x[495,883] 各 389px;左缘 x489 裁切描边、x482–488 纯正文底(左缘不出血);斑马行 y280 faint=155 / 次行 y310 faint=0 交替保持;上方窄表不受影响(y98 faint=125)。
- **禅定(F11,浅色,`/tmp/t30-light-zen.png`)**:禅定栏 ~650px 恰容六列表格(无横向溢出),两表结构完整(窄表 60–199、宽表 215–298,底色/分隔线/斑马全在);圆角 6px 使方形底色比描边线两端各宽 ~5px(角部几何,非异常)。
- **结论:未发现裁切异常,无修复**。合成横向滚轮(button 6/7、shift+wheel)在本机 wgpu 路径不驱动 egui 横向 ScrollArea,中段态经拖拽滚动条达成。

## 5. 修复登记:表头/加粗文字曾呈 accent 蓝(#48)

像素取证过程中发现(先于 #30 存在,非本三模块引入):vendored 渲染在未注册 `"bold"` 字族时对表头/`**加粗**` 回落 `Visuals::strong_text_color()`,而 egui 0.36 该值 = `widgets.active.fg_stroke.color`,#27 的 `apply_shell_to` 把 active 前景设为 accent(`#6C9FFF`)——叠加结果:预览表头、加粗、标题(标题无条件取 strong)全部呈链接同款蓝。修复:app 侧 `fonts.rs` 注册 `"bold"` 族名别名挂 Inter SemiBold 链(decisions-pending #48,vendored 设计文档写明的注册路径),表头/加粗恢复**半粗字重 + 正文色**;标题仍为 accent(上游行为,vendor 改动另行立项)。验收:**两套主题全预览 accent 蓝像素 = 0**(深色 `#6C9FFF` / 浅色 `#3370FF` 判定窗),表头带白字像素 416–501px 可见。

## 6. 对照:预览与导出观感对齐(无需改动)

`latermd-export/src/lib.rs:46-55` 的表格 CSS 已有 `th,td { border: 1px solid #d0d7de }` + `th { background: #f6f8fa }`(暗色 `#161b22`/`#30363d`)——边框 + 表头底色齐全,与本轮预览「1px 边框 + faint 表头底 + 隔行」观感对齐;`#48` 修复后表头文字色亦对齐(导出 `th` 无特殊色)。零改动。

## 7. 结论与遗留

- **明暗两套 × 三断言全部通过**;横向滚动裁切与禅定场景无异常;`#48` 蓝字修复生效。
- 出厂默认(`theme.rs::default_markdown_style`)与九套预设(`theme_presets.rs::base`)两开关为 true 由单测钉住(`default_markdown_style_draws_table_borders` / `builtins_draw_table_borders`)。
- 旧皮肤文件的向后兼容:九套预设按「不存在才写」铺盘,已存在的旧 `.ron` 不含新字段、serde default false 回落(无底色但不报错);本验收用干净目录重铺,取的是含新字段的版本。
- 遗留:① vendored 标题仍呈 accent(§5,#48「如何改」);② 横向滚动条仅悬停显形、合成滚轮事件不可用,真机触摸板横滑体验留人工清单;③ Win/mac 真机表格观感不在本机范围。

## 8. 独立评审修复:底色可见性(2026-09-29 第二轮)

> 独立评审(medium)指出:§2/§3 以「逐像素可分」判 PASS,**只证明画了,不证明看得见**——旧取色源 `theme.rs::shell_tokens().faint` 深色 `#2A2B2E` vs 正文底 `#292A2D` 每通道仅差 1(亮度差 ~0.4%,低于均匀大色块的感知阈,深色模式底色事实上隐形);浅色 `#FAFBFC` vs `#FFFFFF` Δ=(5,4,3) 也弱于导出 CSS 的 th 底 `#f6f8fa`(Δ=(9,7,5))。§2/§3 的数字保留为修复前取证。

**修复**(取值对齐导出 CSS,零 vendor、零新配置面;取色链路不变,仍是 `faint` token → `visuals.faint_bg_color` → vendored `paint_header_fill` + `egui_extras striped`):

| 主题 | faint 旧值 | faint 新值 | 与 content 差 | 取值依据 |
|---|---|---|---|---|
| 浅色 | `#FAFBFC` | **`#F6F8FA`** | Δ=(9,7,5) | 与导出 HTML th 底同值([latermd-export CSS](../crates/latermd-export/src/lib.rs) `th { background: #f6f8fa }`),预览不弱于导出 |
| 深色 | `#2A2B2E` | **`#323438`** | Δ=(9,10,11) | 对齐导出 CSS 暗色分支口径(th `#161b22` vs 正文 `#0d1117`,Δ=(9,10,11)) |

回归防线:单测 `theme::tests::faint_table_fill_is_visible_against_content`——明暗两套 faint 与 content **每通道 |Δ|≥5**(判据从「像素可分」升级为「可感知」)、浅色与导出 th 底同值、投影后 `visuals.faint_bg_color` 与 token 一致。

**像素取证重跑**(同 §1 样例与行带几何,`/tmp/t30-dark.png` / `/tmp/t30-light.png` 覆盖为修复后截图):

| 断言 | 深色实测 | 浅色实测 | 结论 |
|---|---|---|---|
| ① 表头行底色可见 | 窄表表头带 y88–105:faint `#323438` 3041px + 右侧 content 3636px;宽表表头带同构 | 表头带 faint `#F6F8FA` 3044px + content 3636px;宽表 6820px | PASS |
| ② 数据区隔行交替 | alpha faint=3846 / beta faint=**0** / gamma faint=3763 / delta faint=**0**(faint 带精确匹配新值) | alpha 3846 / beta **0** / gamma 3763 / delta **0**;宽表 r1=6268 | PASS |
| ③ 边框线可检出 | 顶 y85=200px、分隔 y112/140/168/196=211px、底 y224=200px;宽表 240/267/295/323=382–389px | 顶/底 200px、分隔 211px;宽表 383–389px | PASS |

深色底色对比:新值 Δ=(9,10,11)/通道(sRGB 线性化后相对 content 亮度 ~+53%),对比旧值 Δ=1/通道(~+5%,恰在 Weber 阈下沿、实测不可辨)——底色在深色模式恢复可见。取证环境备注:锁屏 `dde-lock` 复现两次(kill 后恢复,同 §0.1);另踩一坑——窗口落在 (550,241) 时被其它顶层窗口遮挡,`import -window` 抓帧不受影响但 XTEST 点击被顶层吃掉(表现为输入全无效、帧间零差),`windowmove 0 0` + `windowraise` 后恢复。
