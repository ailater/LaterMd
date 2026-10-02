# 预览排版收口:标题分级与呼吸间距验收(2026-10-02)

> 验收对象:#23 F4(vendored ①类,commit `02b33ad`:标题字号分级 `HeadingStyle::scales` 默认 [2.0,1.55,1.30,1.15,1.08,1.0]、`MarkdownStyle::heading_space_above` 默认 4.0)+ F5(本棒:app 预设同步与验收记录)。
> 方法:**无头自动像素取样** —— `cargo test -p latermd-app --bin latermd preview_typography -- --nocapture`。与 m5-acceptance(`DISPLAY=:0` 启动真机 → `import` 截图 → python3+PIL 全图采样)不同,本轮全程在 egui headless context 里跑生产链路渲染一帧 → 曲面细分(feathering 关)→ **三角形扫描线光栅化**成最终色图层(画家算法,后画覆盖先画),再在图层上做带结构分析;逐字形墨迹中心采样沿用 #43 M2(`preview_pixel_acceptance.rs`)的双 child 手法。
> 之所以走无头:本机 Deepin 无头环境下合成输入不可信(m5 §0.1/table-render §0 的既有结论,xdotool 被 dde-lock 与 fcitx 反复吃键),而本轮验收断言全部是**数值/几何**性质(行高/间距/字号比),不需要交互;真机目视项(§5)单列人工清单。

## 1. 验收配置(与生产完全同源)

| 项 | 值 | 来源 |
|---|---|---|
| 生效样式 | `ThemeSettings::default().apply()` 装进 context 的全局样式 | `theme.rs::apply` |
| 用户行距 | 1.5(出厂默认,**覆盖** vendored 出厂 1.30;滑杆唯一真源) | `theme.rs`(#23 F3 链路) |
| CJK 行高下限 | 本机 Noto Sans CJK face 表值 ≈1.448em(注入 `min_line_height_em`) | `fonts::line_height_floor_em` |
| 字体 | `FontId{size: 15.0, family: 预览专用族 Inter-Preview}`(行 metrics 对齐过 CJK 的副本) | `fonts::preview_body_family` |
| 标题分级 | [2.0,1.55,1.30,1.15,1.08,1.0]×15pt → 30.0/23.25/19.50/17.25/16.20/15.0pt | vendored `style.rs`(F4) |
| 标题呼吸 | `heading_space_above=4.0`,spacer 行高 = `block_spacing(8)+4=12.0` | vendored `layout.rs`(F4) |
| 主题 | 明、暗各一轮(`Visuals::light/dark`,断言用色全部从当帧实际 visuals 回读) | — |

面板 700px 宽,ppp=1,feathering 关(边缘三角形硬边,采样读色不被透明渐变污染,#41 口径)。

## 2. 证据一:排版文档(整篇 galley 路径)—— 精确几何

测试:`typography_doc_geometry_matches_shipped_style_in_both_visuals`
(`preview_typography_acceptance.rs`;样例文档 = 正文段与 H1-H6 交错 + 列表 + 超长 wrap 段,**无块元素**,走整篇 galley 路径 —— LaterMD 预览最常见的文档形态)。明暗两主题数字**逐项相同**(几何与颜色无关),只记一套:

| # | 断言 | 实测数字 | 结论 |
|---|---|---|---|
| ① | spacer 行:6 个中段标题各恰一条,行高精确 = `block_spacing+heading_space_above` | **6 条,每条 12.00px**(8+4) | PASS |
| ②a | 阶梯(布局层精确):各级标题 section 字号 == 15×分级(±0.01) | **30.00 / 23.25 / 19.50 / 17.25 / 16.20 / 15.00 pt** | PASS |
| ②a' | 各级 section 行高 == max(字号×1.5, 字号×CJK 下限+0.75) | **45.00 / 34.88 / 29.25 / 25.88 / 24.30 / 22.50**(= 各字号×1.50;1.5 > 1.448+0.75/size,floor 未顶到) | PASS |
| ②b | 阶梯(字形墨迹层互证,±0.13):同组探针字形「标题分级」在各级行 vs 正文行的 ink 高比 | ink **30 / 23 / 19 / 18 / 17 / 16 px**(正文 16px),比 **1.875 / 1.438 / 1.188 / 1.125 / 1.062 / 1.000** | PASS(注 1) |
| ③ | H1 行盒随字号重算(§1「写死 17px 裁切」回归锚) | H1 行盒 **45.00px** == 30pt×1.5 | PASS |
| ④ | 标题呼吸差值:标题上方空隙 − 段落间空隙 ≈ spacer 高(±1.5) | 标题上方 **35.00px**(空行行 23 + spacer 12)vs 段落间 **23.00px**,差 **恰 12.00px** | PASS(注 2) |
| ④' | 段落空隙均匀性(空行行高一致 → ④ 的推导成立):标题后/正文段后/列表后三种前驱 | 实测 23.00-23.00px,极差 0 | PASS |
| ⑤ | 逐字形墨迹中心像素完整性(#43 同款):全部内容行(含标题/列表)CJK 字形 | 明暗各 **222** 个采样点,最终覆盖者全部是不透明文本色(未被任何背景块/相邻行盖掉) | PASS |

注 1:②b 的探针 ink 比在 15→30pt 上实测 1.875 而非严格 2.000,是**位图光栅化的逐字形 ±1px 取整**(同一探针字形 16px@15pt vs 30px@30pt,em 覆盖率 1.067 vs 1.0),不是布局偏差 —— 布局输入(②a 的 section 字号)是精确的。±0.13 容差下新旧分级仍可判(旧默认 1.6 与新 2.0 差 0.4)。
注 2:④ 的推导结构:标题行上方是「空行行(23px)+ spacer 行(12px)」,段落间是「空行行(23px)」,两者之差即 spacer 高 —— 实测差 12.00px 与 spacer 12.00px **精确相等**(±1.5 容差内的 0 偏差)。

## 3. 证据二:全元素文档(分段渲染路径)—— 最终色图层带结构

测试:`full_sample_doc_elements_visible_and_spaced_in_both_visuals`
(样例文档 = H1-H6 + 列表 + **引用块 + 表格 + 代码块** → 引用/表格把文档切进**分段渲染路径**;`show()` 整链渲染 → 光栅化 → 逐像素分析)。明暗两主题:

| # | 断言 | 实测(明暗同构同坐标) | 结论 |
|---|---|---|---|
| ① | strong 色带恰为 6 级标题(app 的 dark visuals 下 strong_text_color=#6C9FFF 即标题色;表头走 bold 字族+正文色,#30 口径) | **6 条** | PASS |
| ② | 标题阶梯(像素带高):H1 带高 ≈ 2× 各低阶带(±0.2,带高有 ±1-2px 字形集噪声) | H1 **30px**/H6 **16px**(1.875)、H2 23px(1.438)、H3 19px、H4 18px、H5 17px | PASS |
| ③ | H1/H6 与正文带的分离下限(旧默认 1.6 下 H1/正文 ≈1.4;新 2.0 下 ≥1.7) | H1/正文 **30/16=1.875 ≥1.7**;H6/正文 16/16=1.000(±0.2) | PASS |
| ④ | 标题呼吸(分段路径同样生效):标题带上方空隙 > 标题下正文带空隙 | 标题上方 **41-46px** vs 标题下 **29-33px**,最小值差 8px 以上;像素差 17.0px(= spacer 12.0 + 行盒内墨迹空隙不对称项,H1 行盒 45px vs 正文 22.5px;护栏 [12,36] 内) | PASS(注 3) |
| ⑤ | 引用竖条/表格竖线:竖直 border 色列(≥16 个 border 像素/列) | **6 根**竖列(#3C4043(暗)/#E5E6EB(亮)) | PASS |
| ⑥ | 表格横边框:≥3 条 ≥150px 宽的 border 色 y 行 | **6 条**(顶线/表头分隔/底线 + egui_extras 网格线,首条 y=795) | PASS |
| ⑦ | 代码块底色大矩形(≥200×30px,表格之下) | **694×45px**(#404040(暗)/#E6E6E6(亮)= `code_bg_color`) | PASS |
| ⑧ | 全元素行带量:≥18 条文本带(7 正文段+6 标题+2 列表+1 引用+表头+表格行+代码 3 行+文末) | **22 条** | PASS |

注 3:分段路径下标题与正文在同一 flush 段内,呼吸结构与整篇路径一致(空行行+spacer);像素级空隙差 = spacer + 两侧行盒内空隙的**不对称项**(ascent>descent,H1 行盒内空隙 15px vs 正文 7.5px,两侧不同号的行盒内空隙差不能抵消),因此像素层只做量级护栏 [spacer, 3×spacer];spacer 的**精确**呼吸算术由证据一的行盒层测量承担(§2④)。
光栅化层的明暗两套实测色对账:暗色层 y>736 区域(引用/表格/代码块)共 8 种不透明色 —— 正文墨 #E8EAED(11398px)、代码块底 #404040(30746px)、边框 #3C4043(2622px)、faint 底 #323438(9748px)、syntect token 色 4 种(#C0C5CE/#8FA1B3/#A3BE8C/#B48EAD);亮色层**逐一同构**(#1F2329 正文墨/#E6E6E6 代码底/#E5E6EB 边框/#F6F8FA faint + 同组 4 种 token 色),像素数同量级 —— 无一种结构色仅在单主题出现,全部元素在两主题下都真实着墨。

## 4. 证据三:正文观感否决线(app 生效样式链路镜像)

测试:`body_only_document_is_invariant_to_heading_space_on_app_style`
(纯正文文档,行数与总高度对 `heading_space_above` 完全不变;vendored 侧已在 vendored 默认样式上钉过 `tests/heading_spacing.rs::body_only_document_height_is_invariant_to_heading_space`,这里在 **app 生效样式链路**(用户行距 1.5 覆盖 + CJK 下限 + 预览专用族)上再钉一次):

| heading_space_above | rows | 总高度 |
|---|---|---|
| 0.0 / 4.0 / 40.0 | 5 / 5 / 5 | 115.00 / 115.00 / 115.00px(差 <0.01) |

**PASS** —— 用户把标题呼吸调到 0 或 40,一个字的正文排版都不动。这是 preview-typography §1.2「正文观感不变」口径(preview-typography §2.3 验收④)在 app 配置下的否决线证据;出厂 `line_height_ratio` 维持 vendored 1.30 未动,app 侧用户默认 1.5 是 F3 既有决策。

## 5. 真机目视项(blocked_external —— 最终判据,眼睛说了算)

以下各项**无头断言只覆盖了几何与可见性**,「好不好看」必须真机/真窗口目视。本机为 Deepin headless(合成输入不可信的既有结论,见 §0 引),三平台真机清单:

| # | 项 | 判据(preview-typography §2.3) | 无头侧已覆盖的部分 |
|---|---|---|---|
| M1 | H4-H6 肉眼可辨 | 14pt 正文下 H4≈15.0/H5≈14.0/H6=13.0pt 的 0.8-1.0pt 级差在真实 AA 渲染下是否能分辨层级(无头只验了字号比值 1.125/1.062/1.000 精确落档) | §2② |
| M2 | 标题呼吸感 | 标题上方总呼吸(空行 23px+spacer 12px=35px)在观感上是否「明显大于」段落间空隙(23px),且不过分(数值比 1.52:1 已钉) | §2④ |
| M3 | **正文密度不变(否决线)** | 与本轮改动前的 build 对比看同一篇纯正文文档,观感上无任何变化(行数/高度/行距逐像素不变已钉,但「观感不变」仍需对照) | §4 |
| M4 | H1 2.0× 是否过大 | 26-30pt H1 在真实窗口(900px 宽三栏布局,预览栏仅 ~420px)下是否压迫感过强;微调口径:改 vendored 默认 `heading_space_above` 或 app 侧 `theme_presets::base()`(见 §6) | §2②a |
| M5 | 明暗两主题实际观感 | 各截一张样例文档渲染图目视(无头已验明暗两套均无裁切/遮挡) | §2⑤/§3 |
| M6 | 三平台字体差异 | Win11 雅黑/macOS PingFang 的 ascent/descent 与 Noto 不同,override 目标随平台字体自适应但未真机验证(#43 遗留) | — |

## 6. 微调口径(真机目视后如何改)

| 想调什么 | 改哪里 | 影响面 |
|---|---|---|
| 标题呼吸(全局) | vendored `egui_markdown_style/style.rs` 的 `default_heading_space_above()`(vendored 默认)+ `theme_presets::base()`(九套预设,两处**必须同改** —— 有 `builtins_ship_heading_typography_matching_factory` 测试钉住一致) | 皮肤 ron 已存在的老安装**不会**被顶掉(install_to「已存在不写」),他们继续用旧值;serde default 只兜**缺字段**的旧档 → 新值只对新装/删了皮肤目录的安装生效 |
| 标题呼吸(单皮肤) | 手改 `themes/<name>.ron` 的 `heading_space_above` 字段 | 仅该皮肤 |
| 标题分级 | vendored `HeadingStyle::default()` 的 scales(①类,独立 vendor: commit) | 同上;`theme_presets` 的交叉断言会红,需同步 |
| 正文行距 | 外观页滑杆(用户值,**出厂 1.5**,与标题间距正交 —— 见 §7) | 用户设置 |
| spacer 语义(「叠加」→「替换」) | vendored `layout.rs` heading 分支 spacer 行高去掉 `block_spacing` 项(decisions-pending #74「如何改」) | vendor ①类 |

## 7. 正交性与兼容性(单测证据)

| 测试 | 断言 | 位置 |
|---|---|---|
| `line_height_slider_and_heading_space_above_are_orthogonal` | 滑杆行距(1.2-2.0 全档)怎么动,皮肤里的 `heading_space_above`(11.0)不被吞;皮肤把标题间距改 0-40 全档,行距保持滑杆值 1.7 不动;出厂链路下 vendored 新默认 4.0 穿透到生效样式 | `theme.rs` 测试 |
| `builtins_ship_heading_typography_matching_factory` | 九套预设 `heading_space_above`==4.0、scales==新分级,且与 `default_markdown_style()`(未选皮肤时)逐项一致 —— vendored 默认将来再变时此测试红=强制显式三处同步 | `theme_presets.rs` 测试 |
| `installed_skins_carry_heading_typography_fields` | 铺盘落档的九个 `.ron` 里显式写有 `heading_space_above: 4.0` 与新分级;`SkinCatalog` 载回后新字段保持 | `theme_presets.rs` 测试 |
| vendored `heading_space_above_defaults_when_missing_and_keeps_explicit_values` | 老安装旧 ron(缺字段)serde default 兜底 4.0;显式 `0.0` 是合法偏好原样保留 | vendored style crate 测试(F4 落) |

兼容行为(与 preview-typography §4 待办 C 预案一致):`install_to` 语义是「**已存在才不动**」—— 九套预设 skin 目录里已有的 `.ron`(用户改过的、或旧版铺的)**不会被新出厂值顶掉**,这些皮肤继续用旧 ron 里没有该字段时 serde default 兜底的 4.0(vendored 默认值)。皮肤系统由此拿到的普通 `.ron` 文件可被用户改、改了不会在下次启动被出厂值顶掉,是该机制的既有语义(且已有 `install_writes_once_and_files_load_as_skins` 回归钉住)。

## 8. 与 vendored 侧测试的分工

vendored `tests/heading_spacing.rs`(F4 落,10+1 例)在 **vendored 默认样式**(13pt/ratio 1.30/无 CJK 下限/egui 出厂字体)上钉住:spacer 行高三档精确值、总高度随标题数线性、文档/段首标题不插 spacer、纯正文零影响、整 galley 与 segmented 两条路径都生效、块后标题保守语义、inline 切片单 spacer。
本轮(`preview_typography_acceptance.rs`,3 例)在 **app 生产生效链路**(15pt/用户行距 1.5/CJK 下限/Inter-Preview 副本/明暗 visuals)上钉住:同一组机制在生产配置下的数值、像素层可见性、否决线镜像。两层互为印证,数值不同的项(行高 45.00 vs 29.25@H1 等)全部可由两套配置差异推出。

## 9. 结论

- 自动可验的部分(8 类数值断言 × 明暗两主题):**全部通过**,数字落档于 §2-§4。
- 否决线(正文观感不变):无头侧已钉死(§4);真机对照目视(M3)留人工。
- 最终观感判据(M1-M6)留真机目视清单,**眼睛说了算之前本验收不宣称「好看」只宣称「数值如设计」**。
