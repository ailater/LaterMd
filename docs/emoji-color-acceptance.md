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

---

# E-C2 收口:预览内联 emoji 彩色自动/人工验收(#48,2026-10-04)

> 验收对象:#48 E-C2「预览内联彩色 emoji」——B1 渲染副本改写器(`b50a292`)+ B2 inline widget 接线(`a6ea1bc`)已落,本文档为 B3 验收落档(纯 docs 模块,零代码改动)。
> 方案依据:[emoji-color-feasibility.md](../.zcode/workflow-drafts/emoji-color-feasibility.md) §3(改写 + inline_widget 两件套、§3.2 点名的 heading/table 断言缺口、§3.3 豁免与偏移纪律)与 §7(E-C2 定义);实现岔路已登记 decisions-pending #90(B1 三口径)/ #91(B2 两口径)。
> 证据分两类:**自动验证** = 本机实跑(§1–§3,命令与输出均为 2026-10-04 实测);**人工目视** = §4 清单,本机 Linux 无头环境无法自验,全部标「待坤哥」,不以单测冒充目视。
> 口径红线(E-C1 口径按交付面扩展):**面板与预览内联彩色;编辑器正文(含 Live 模式富渲染块)仍黑白;导出/外发的显示效果取决于目标环境的字体。** 不写「全面支持彩色」。

## 0. 范围与口径(先读这个)

- **做了**:右栏预览的段落 / 标题 / 列表 / 引用 / 表格单元格里的**覆盖集内** emoji(与面板同一份 `emoji_data` 全表 272 枚,`covered_glyphs()` 注入,`emoji_data.rs:279`)显示为 Twemoji 彩色。机制是两件套,缺一不可(feasibility §3.2 的结论):
  - **B1 改写器**:`latermd_md::expand_emoji_links` 纯函数(`latermd-md/src/lib.rs:740`)把渲染副本里的裸 emoji 改写为 `[😀](<emoji://😀>)`;豁免区间走**与渲染同一套 pulldown-cmark**(同 vendored options)的事件区间——围栏/缩进代码块、行内代码、既有链接的文本与目标、HTML 块与行内标签、脚注;同次产出第二层 `OffsetMap`(`state.rs:179` `emoji_map`,wikilink → emoji 两层串行可组合,#14 LP2-4 口径)。
  - **B2 inline widget**:`AiLinkHandler` 接 vendored `LinkHandler` 五级扩展点(app 侧实现,vendor 零改动):`inline_widget_size`(`preview.rs:409`,font.size 正方形,行高恒不超正文自然行高)/ `layout_link`(`preview.rs:382`,透明占位 = 链接文字本体 + 同款字体,推进宽度与普通文本逐像素一致)/ `paint_inline_widget`(`preview.rs:420`,查 `emoji_panel::inline_texture` #47 同源会话缓存 → 白 tint 画方块;查不到什么都不画)/ `link_style`(`preview.rs:329`,正文色 + 无下划线)/ `click`(`preview.rs:351`,吞掉,不交系统浏览器)。
- **明确不含**:编辑器正文彩色(feasibility §4 否决:需 fork epaint)、**Live 模式富渲染块**(`live.rs:457` 直接 `MarkdownLabel` 渲染源码 block_text,不进改写链、不挂 handler——Live 是编辑器形态,同「正文黑白」口径)、代码块/行内代码内的 emoji(B1 豁免,保持黑白字形)、导出 HTML 的任何「保证彩色」承诺(feasibility §5)。
- **已知边界(如实,不硬撑全覆盖)**:
  - 标题里的 emoji **不随 H1–H6 字号放大**:vendored 层对 `Token::Link` 一律传正文基础字体,与 wiki:// / ai:// 链接文本在标题里的既有行为同源,非 B2 引入的回归——断言把它钉成已知如实行为(decisions-pending #91,含 vendor ①类改法)。
  - 覆盖集外字符(如 🫠)不改写(#90:只有 272 枚有 #47 资产,改写而无纹理 = 该 emoji 从预览消失);手写表外 `emoji://` 链接 → 透明占位不动,该处不显示字形。
  - 链接引用定义 `[😀]: url` 的标签、脚注定义正文内的 emoji 暂不改写(黑白)——#90 登记的已知边界(极罕见形态)。
- **失败面**:纹理缺失 / 解码失败 → 什么都不画,透明占位原样保持(不画黑块不 panic,连续两帧复测);改写零外泄——源码、rope、修订号、dirty、撤销栈、落盘字节分毫不动(§1 ③)。

## 1. 自动验证证据(本机实跑,2026-10-04)

### ① 三段覆盖断言(可行性调查 §3.2 点名的缺口)—— PASS

命令:`cargo test -p latermd-app --all-features emoji_inline_widget`

输出:`test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 620 filtered out`

| 段 | 测试(`ui::preview::tests`) | 钉住的事实 |
|---|---|---|
| 段落 | `emoji_inline_widget_paints_textured_squares_in_paragraph` | 两枚 emoji 各留一个 widget 区块、各画一张纹理图;mesh 绑定的就是本帧上传的 72×72 Twemoji 纹理(`textures_delta` 引擎层证据);正方形边长与正文字号同源(0.8–3 倍区间)、纵向落在正文行带 ±1px 内、按文档序左右排开;正文文字照常在文本层 |
| 标题 | `emoji_inline_widget_paints_inside_heading_at_link_font_scale` | 图片纵向落在「标题」文本同一行带内(**heading 吃到 inline widget**);边长与段落档一致(链接字体不吃 heading 缩放,#91 已知如实行为,非漏网) |
| 表格 | `emoji_inline_widget_paints_in_table_cell` | 表格 cell 走 vendored 另一条 `render_link_in_ui` 每链接 widget 路径,同样画成纹理正方形;表头与正文文字照常渲染 |

> §3.2 缺口的销账结论:**三段全部吃到 inline widget,无需「该段暂为黑白」的降级登记**;唯一降级口径是「标题内不随字号缩放」(上表 #91 行)。

### ② 豁免测试(B1 保证、B2 渲染层复测)—— PASS

改写层命令:`cargo test -p latermd-md --all-features expand_emoji_links`

输出:`test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 40 filtered out`

| 断言 | 测试(`latermd_md::tests`) |
|---|---|
| 围栏/缩进代码块 + 行内代码豁免 | `expand_emoji_links_skips_code_blocks_and_inline_code` |
| 既有链接的文本与目标豁免(防嵌套破坏) | `expand_emoji_links_skips_existing_link_text_and_destinations` |
| HTML 块/行内标签 + 脚注豁免 | `expand_emoji_links_skips_html_and_footnotes` |
| 改写发生面(正文/标题/列表/引用) | `expand_emoji_links_rewrites_body_heading_list_quote` |
| 与 wikilink 层叠加互不破坏 | `expand_emoji_links_stacks_after_wikilink_expansion` |
| 无命中恒等 / 幂等 / CJK 与多字符边界 / 映射换算 | `…_identity_without_covered_hits` / `…_is_idempotent_on_own_output` / `…_cjk_boundaries_and_multichar_glyphs` / `…_map_translates_offsets` |

渲染层复测(任务书「豁免由 B1 保证,B2 复测一条」):`emoji_dense_document_renders_both_themes_without_panicking`——明暗两主题 × 连续两帧 × 密集文档,断言**恰 29 枚**进 widget / 画图(正文 6 行 ×4 + 标题 1 + 列表 2 + 引用 1 + 表格 1),代码块 `let e = "😀";` 与行内 `` `🚀` `` 保持字面文本、不进 widget 不多画图。

### ③ 落盘字节不变(改写零外泄)—— PASS

命令:`cargo test -p latermd-app --all-features emoji_rewrite`

输出:`test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 621 filtered out`

| 断言 | 测试(`state::tests`) | 证据口径 |
|---|---|---|
| 盘上字节 == 源码 | `emoji_rewrite_never_leaks_into_saved_bytes` | emoji 文档建预览快照(前置断言 `rendered != source`,断言非恒真)后 `save_to`,读回逐字节相等——改写只活在渲染副本,保存走编辑缓冲 |
| 源码零外泄 + 两层映射 | `preview_emoji_rewrite_touches_only_rendered_copy` | 缓冲/快照真源/修订号/dirty 分毫不动;wikilink→emoji 两层串行穿过,标题偏移落渲染串同一文本处且逆穿回源码原处 |

配套:`covered_glyphs_round_trip_through_emoji_links`(272 枚全表经改写器往返可逆,含在 ④ 的 emoji 过滤轮里)、`map_source_offset_through_both_rewrites`(三层串行穿透,`preview.rs:129` `map_source_offset`;测试名不含 "emoji",单独跑:`cargo test -p latermd-app --all-features map_source_offset_through_both_rewrites` → `1 passed`)均过。

### ④ 副作用压住 + 回落 + 回归 —— PASS

命令:`cargo test -p latermd-app --all-features emoji`

输出:`test result: ok. 50 passed; 0 failed; 0 ignored; 0 measured; 573 filtered out`

| 断言 | 测试 |
|---|---|
| link_style = 正文色 + 无下划线;click 吞掉且零消息;普通链接样式/放行分毫不动 | `emoji_link_style_and_click_are_contained` |
| 手型抑制双向:悬停 emoji = Default,悬停普通链接仍 PointingHand | `emoji_hover_keeps_default_cursor_while_links_keep_pointing_hand` |
| 纹理缺失(表外手写 `emoji://`)两帧占位不动、不画黑块不 panic、周边文本照常 | `emoji_texture_miss_keeps_placeholder_without_panicking` |
| 改写后整档明暗双主题无头渲染不 panic(代码块字面保留) | `preview_ui_renders_emoji_rewritten_doc_without_panic` |

### ⑤ 全量回归 —— PASS

命令:`cargo test -p latermd-app -p latermd-md --all-features`

输出:latermd-app `test result: ok. 622 passed; 0 failed; 1 ignored`(ignored 为 #39 既有取证)、latermd-md `test result: ok. 49 passed; 0 failed`。

## 2. 纹理缓存与显存(引用 E-C1 §2,增量口径)

- 预览与面板**同一份会话缓存**:`emoji_panel.rs:415` `inline_texture`,同一 temp memory 键——面板先开则预览直接命中,反之亦然,同一 Context 内每枚至多解码一次(E-C1 §1 ① 的命中计数承诺不变,消费方 +1)。显存上限口径不变:272 枚全量常驻 ≈ 5.6 MB(E-C1 §2),预览不新开缓存、不合并图集。
- 每帧成本:命中走只读 `data` + 句柄克隆(emoji 密集文档的常态);未命中的解码与 `load_texture` 在 data 锁外做(egui 0.36 `load_texture` 经 `Context::input` 再入写锁,包在 `data_mut` 闭包里会自锁死——#48 B2 开发期实测,注释在案 `emoji_panel.rs:411-414`)。

## 3. 对外口径核对 —— PASS(2026-10-04 逐项核对)

| 落点 | 现状(B3 逐字/逐项核对) |
|---|---|
| 禁用语「全面支持(彩色)」 | B3 编辑后重跑 `grep -rn "全面支持" docs/ crates/ README.md`:命中 6 处,**全部是禁令/核对记录自身**(E-C1 红线与 §3 核对、E-C2(本文)红线与本行、emoji-plan §11/§12 禁令),无一处能力宣称;`crates/`、README.md 零命中 |
| [emoji-plan.md](emoji-plan.md) | §2 F2 连带结论已随 E-C2 补记「预览内联彩色」;新增 §12 E-C2 落地登记(机制/范围/已知边界/口径);§11 的「另案」处补指向 §12 |
| 导出链路 | `latermd-export` 零改动(B1/B2 提交 `git show --stat` 均未触碰 `crates/latermd-export`),emoji 以 Unicode 原样透传;口径 = 显示效果取决于目标环境的字体(feasibility §5),不承诺任何平台必然彩色 |
| 面板底部说明行 | `emoji_panel.rs:619` 仍为「面板内彩色;编辑器正文仍黑白;导出/外发的显示效果取决于目标环境的字体」——句子仍真(面板确实彩色)但**未提预览**,属欠完整而非错误;文案在 `crates/`,B3 模块 paths=docs 不动,登记 §5 遗留跟进 |
| 偏移锚点 | 大纲跳转/section anchor 的偏移换算升级为三层串行穿透(`preview.rs:126-139`),#14 LP2-4 既有口径不破坏(`map_source_offset_through_both_rewrites` 钉住) |

## 4. 人工验收清单(blocked_external)

**缺什么**:本机为 Linux 无头环境,无法开窗口目视;无 Windows 11 / macOS 14 真机。自动测试证明「数据与管线对」(纹理上传、mesh 发出、占位几何、豁免边界),不能证明「眼睛看到了彩色且观感可接受」。

| # | 项 | 判据 | 状态 |
|---|---|---|---|
| M1 | Linux 真机预览正文彩色观感 | 正文/列表/引用里的 emoji 为彩色 Twemoji,与文字基线、行距、行高无漂移错位,无「顶高行」 | 待坤哥 |
| M2 | 标题与表格中的彩色 emoji | H1–H6 与表格 cell 内 emoji 彩色;已知「标题内不随字号放大」的观感可接受与否由坤哥裁决(#91 附 vendor ①类改法) | 待坤哥 |
| M3 | HiDPI 缩放 | 1x 与 2x 缩放下预览 emoji 无模糊/拉伸/错位(72px 资产对 font.size 格子 ≥1:1 采样) | 待坤哥 |
| M4 | 明暗主题 | 两主题下彩色 emoji 清晰可读,暗色下不发灰、亮色下不刺眼 | 待坤哥 |
| M5 | 代码区黑白对照 | 同文档代码块/行内代码里的 emoji 仍为黑白字形(豁免),与正文彩色形成预期对照,无「以为坏了」的落差 | 待坤哥 |
| M6 | 面板 ↔ 预览缓存同源 | 先开面板再滚预览(或反之)无重复解码卡顿;emoji 密集文档滚动流畅 | 待坤哥 |
| M7 | Win11 / macOS 14 预览目视 | 打包版预览彩色正常(资产 `include_bytes` 随二进制走,机制上无平台差异,以目视为准) | 待坤哥 |
| M8 | 导出/外发抽查 | 同文档导出 HTML:现代浏览器预期彩色、纯文本查看器黑白——「取决于目标环境的字体」话术与事实相符 | 待坤哥 |

## 5. 结论与遗留

- **自动侧全部通过**:三段覆盖断言 3/3、改写层豁免与行为 9/9、落盘字节不变 2/2、副作用/回落/回归全绿(§1 命令与输出均可复现)。
- **heading/表格断言都过了**——无需把任何一段降级为「暂为黑白」;唯一缩水口径是「标题内 emoji 不随字号放大」(#91 已知如实行为)。
- **人工侧 §4 八项全部待坤哥真机目视**,销账前本功能按口径只宣称「预览内联彩色已落地(无头证据)」,不宣称全面完成;Win/mac 两项在真机到位前保持 blocked_external。
- 无头测试不冒充目视:三段断言与 mesh 取证是引擎层证据(feasibility §8 同口径)。
- **遗留跟进**(均不阻断销账):① 面板底部说明行 `emoji_panel.rs:619` 可更新为「面板与预览内联彩色…」(crates/ 文案,后续 commit 顺手带上);② 标题内 emoji 随字号放大需 vendor ①类补丁(#91「如何改」);③ 链接引用定义标签/脚注定义正文的豁免补齐(#90 已知边界)。
