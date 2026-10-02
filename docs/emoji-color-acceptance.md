# E-C1 收口:emoji 面板彩色自动/人工验收(#47,2026-10-02)

> 验收对象:#47 E-C1「插入 Emoji 面板 + 最近使用条彩色」——A1 资产与许可登记(`596ad4c`)+ A2 纹理分支(`f65afe1`)已落,本文档为 A3 验收落档。
> 方案依据:[emoji-color-feasibility.md](../.zcode/workflow-drafts/emoji-color-feasibility.md)(2026-09-30 调查,§2 纹理路径 / §6 许可比较 / §7 推荐方案;该文件在 `.zcode/workflow-drafts/`,未入 git 库,结论摘要已转写进 [emoji-plan.md](emoji-plan.md) §2/§11)。
> 证据分两类:**自动验证** = 本机实跑(§1–§3,命令与输出均为 2026-10-02 实测);**人工目视** = §4 清单,本机 Linux 无头环境无法自验,全部标「待坤哥」,不以单测冒充目视。
> 口径红线:**面板内彩色;编辑器正文仍黑白;导出/外发的显示效果取决于目标环境的字体。** 不写「全面支持彩色」。

## 0. 范围与口径(先读这个)

- **做了**:插入 Emoji 面板 8 分类网格、跨分类搜索结果、「最近使用」条——272 枚全部以随包 Twemoji 72px PNG 纹理彩色显示(白 tint Image,绘制单点分支见 `crates/latermd-app/src/ui/emoji_panel.rs` 顶部「A2 纹理路径」节与 `glyph_cell`)。
- **明确不含**:预览内联彩色(#48 E-C2,走 `emoji://` 改写 + inline_widget,不经字形管线)、编辑器正文彩色(feasibility §4 已否决:需 fork epaint,代价如实列出)、导出 HTML 的任何「保证彩色」承诺(feasibility §5:取决于目标环境字体)。
- **失败面**:PNG 解码失败 / 资产缺失 → 该单元回落出厂 NotoEmoji 黑白字形;与 E3「数据保留、渲染兜底」同哲学,彩色不引入新失败面。缓存 = 会话级懒解码 `TextureHandle`(egui temp memory,面板首帧建、Context 销毁才清),不合并图集(首版不做)。

## 1. 自动验证证据(本机实跑)

### ① 测试:渲染不 panic + 纹理命中计数 + 损坏回落 —— PASS

命令:`cargo test -p latermd-app --all-features emoji_panel`

输出:`test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 548 filtered out; finished in 0.08s`

关键断言拆解(测试名 → 钉住的事实):

| 断言 | 测试(`ui::emoji_panel::tests`) | 证据口径 |
|---|---|---|
| 渲染不 panic | `panel_renders_in_both_visuals_without_messages` | 明暗两套 visuals 各 3 帧(首帧字体注册、后续帧 tessellation 各有冷启动路径),零自发消息 |
| 纹理命中计数 | `panel_loads_one_texture_per_displayed_glyph_and_reuses_it` | 首轮每个 distinct 实显枚**恰好上传一张 72×72 纹理**(`textures_delta` 引擎层证据,排除字体图集假阳性),画面发出绑定这些纹理的 image mesh ≥ 实显枚数;第 4 帧零新增上传 = 会话缓存命中不重复解码。明暗两套各跑一遍 |
| 损坏/缺失回落 | `corrupt_or_missing_assets_fall_back_without_panic` | 坏字节 / 空字节 / 截断真 PNG 解码均 `None` 不 panic;表外字符(🫠)走黑白回落路径,单元照常渲染可点、**不产生任何纹理**、点选载荷仍是原始 Unicode |
| 资产全量在库 | `bundled_assets_cover_the_whole_table_and_decode` | 272 枚与数据表 1:1(`include_bytes` 编译期钉住,挪走/改名即编译红)、无重复、逐枚解码为 72×72 |
| E3 过滤回归 | `factory_font_cmap12_prunes_exactly_the_text_presentation_entries` + `probe_glyphs_covers_table_on_factory_chain` | 出厂 NotoEmoji cmap fmt12 剔除清单**恰好**是 7 枚文本表现条目;彩色化未动 E3 白名单(`probe_glyphs` 零改动) |
| E1/E2 链路回归 | `clicking_a_cell_and_switching_group_emit_messages` / `search_filters_the_grid_across_groups` / `search_result_click_inserts_the_hit` / `recent_row_hides_when_empty_and_inserts_on_click` / `escape_closes_the_panel` / `glyph_filter_prunes_grid_and_shows_placeholder` / `glyph_filter_prunes_the_recent_row`,及 `state::tests::emoji_panel_toggle_keeps_group_clears_query` | 点选载荷 = 原始字符、搜索三路跨分类、最近使用去重封顶与插入、Esc 关面板、开面板清搜索词——插入链路行为与彩色化之前一致 |

### ② 资产在位与可复现 —— PASS

命令:`bash assets/emoji/twemoji/fetch.sh --verify`

输出:`核对: 272 枚, 总体积 208646 B` / `全部 272 枚在位且为合法 72x72 PNG。`

- 来源登记:[SOURCES.txt](../assets/emoji/twemoji/SOURCES.txt)——jdecked/twemoji **v17.0.3**(2026-06-01 release,覆盖 Unicode 17 / Emoji 17),钉版本 URL 逐枚下载,下载日期 2026-10-02,重跑脚本可逐字节复现(该次入库时已实测聚合 sha256 一致)。
- 许可:CC-BY 4.0 全文随库 [LICENSE-GRAPHICS](../assets/emoji/twemoji/LICENSE-GRAPHICS);义务 = 署名 + 附许可文本,第三方声明登记在 [distribution.md](distribution.md) §6(「本表即署名」口径),面板/关于无「自制图标」类误导文案。

## 2. 体积与显存汇总

| 项 | 数字 | 说明 |
|---|---|---|
| 磁盘随包增量 | **208,646 B ≈ 0.20 MiB**(272 枚 PNG) | 单枚均值 767 B、最小 182 B、最大 3,309 B(`find -printf '%s'` 实测);可行性调查口径 ~806 B/枚、0.3 MB 级,实测略优 |
| 二进制增量 | 同上 ≈ 209 KB | `include_bytes!` 原样嵌入二进制,不压缩 |
| 显存上限(全量常驻) | **272 × 72² × 4 B = 5,640,192 B ≈ 5.6 MB** | 每枚解码后为 72×72 RGBA8 未压缩纹理(20,736 B/枚);272 枚全部进过面板且同会话不退出才达到此上限;PNG 209 KB → 5.6 MB ≈ 27× 属未压缩纹理固有膨胀 |
| 懒加载常驻(首屏) | 表情分类 **33 枚** × 20,736 B = 684,288 B ≈ 0.68 MB | 打开面板只解码当前分类网格 +「最近使用」(≤16 枚,多与网格重叠);命中计数测试保证每枚会话内恰解码一次,不重复驻留;会话级缓存,Context 销毁整体回收 |
| HiDPI 密度 | 32 逻辑点格子里画 72px 资产:ppp=1 → **2.25×**,ppp=2 → 64 物理像素 vs 72px 资产仍 **1.125×** 超采样 | `TextureOptions::LINEAR`(`emoji_panel.rs:387`),两档均 ≥1:1 采样,无放大模糊;最终观感归 §4 目视 |

> 分组计数核对(供上表「首屏」口径):表情 33 / 手势 18 / 人物 22 / 动物与食物 40 / 物品 39 / 符号 40 / 旅行 40 / 旗帜 40,合计 272,与资产表、`emoji_data::GROUPS` 三方一致。

## 3. 对外口径核对 —— PASS

| 落点 | 现状(A3 逐字核对) |
|---|---|
| 面板底部说明行 | `crates/latermd-app/src/ui/emoji_panel.rs:592` 逐字 = **「面板内彩色;编辑器正文仍黑白;导出/外发的显示效果取决于目标环境的字体」**(A2 已改,本模块核对) |
| emoji-plan.md §2 | 「F2 的连带结论」已同步同口径:导出行改「显示效果取决于目标环境的字体,不承诺必然彩色」,并记「现行口径」原文 |
| 禁用语 | 「全面支持(彩色)」作为**支持性声明**零命中(docs/、crates/、README.md;命中的 3 处均为禁令/核对记录自身——本文 §0 与此行、emoji-plan §11 的「不许出现」,无一处是能力宣称);`crates/` 内 UI 字符串含「彩色」的仅 `emoji_panel.rs:592` 一处,文案即本口径(另一处命中为测试断言消息,非用户可见) |
| 第三方声明 | [distribution.md](distribution.md) §6 Twemoji(jdecked v17.0.3)CC-BY 4.0 登记行在位,义务 = 署名 + 附许可文本 |
| roadmap.md 旧口径 | 「应用内黑白/导出彩色」两处旧话术(`roadmap.md:29`/`:31`)随本模块订正为「面板/最近使用条彩色(E-C1)、编辑器正文仍黑白、导出/外发取决于目标环境字体」 |

## 4. 人工验收清单(blocked_external)

**缺什么**:本机为 Linux 无头环境,无法开窗口目视;无 Windows 11 / macOS 14 真机。自动测试只证明「数据与管线对」(纹理上传、mesh 发出、回落路径通),不能证明「眼睛看到了彩色且不难看」。

| # | 项 | 判据 | 状态 |
|---|---|---|---|
| M1 | Linux X11 面板彩色观感 | 工具条按钮开面板:网格、搜索结果、最近使用条均为彩色 Twemoji(非黑白、非豆腐),明暗两主题各看一遍 | 待坤哥 |
| M2 | Linux HiDPI 缩放 | 1x 与 2x 缩放下 emoji 无模糊/拉伸/错位,与网格对齐;72px 资产在 32 逻辑点格子里的观感可接受 | 待坤哥 |
| M3 | Win11 / macOS 14 面板目视 | 打包版开面板彩色正常(资产 `include_bytes` 随二进制走,机制上无平台差异,按机制推断一致,以目视为准) | 待坤哥 |
| M4 | 272 枚抽查 | 8 分类逐页翻完(旗帜类 40 枚细看):无豆腐块、无拉伸错位、hover tooltip 三名一路正常、点击插入正确 | 待坤哥 |
| M5 | 编辑器正文黑白对照 | 从面板插入 emoji 后,正文字形仍为出厂 NotoEmoji 黑白轮廓——与底部说明行口径一致,无「以为坏了」的落差 | 待坤哥 |
| M6 | 导出/外发抽查 | 同文档导出 HTML:现代浏览器(有彩色 emoji 字体)预期彩色、纯文本查看器黑白——验证「取决于目标环境的字体」话术与事实相符 | 待坤哥 |

## 5. 结论与遗留

- **自动侧全部通过**:14/14 测试全绿(`cargo test -p latermd-app --all-features emoji_panel`)、272/272 资产校验过(`fetch.sh --verify`)、口径逐字核对过(§3)、体积/显存数字齐(§2),全部可复现。
- **人工侧 §4 六项全部待坤哥真机目视**,销账前本功能按口径只宣称「面板内彩色已落地」,不宣称全面完成;Win/mac 两项在真机到位前保持 blocked_external。
- 无头测试不冒充目视:命中计数与 image mesh 断言是引擎层证据(feasibility §8 同口径:「上述是数据与管线事实,不是『看到了彩色』」)。
