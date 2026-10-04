# ADR-006: PDF 导出技术路线

日期: 2026-10-04
状态: **草案(待用户追认)** —— 按 decisions-pending #96 以默认推荐路线继续,用户翻该条目可推翻
关联: [[adr-001-gui-and-architecture]]、[[adr-004-technical-stack]]、[[adr-003-renderer-and-ecosystem-audit]]

> 本 ADR 只做选型与决策,不含实现。所有第三方数据取自 2026-10-04 当天的
> `cargo search` / `cargo info` / crates.io API / GitHub API / 官方 README,
> 编译与 CJK 探针在本机(rustc 1.98.0,Deepin Linux)实测;凡未核实的维度如实标注。
> 实测探针工程位于 /tmp(不入库),复现方法见 §6.4。

---

## 1. 决策摘要

| 议题 | 结论 |
|---|---|
| 技术路线 | **A:纯 Rust 直绘**(Markdown token → 自建布局 → PDF 后端),否决 B(HTML→PDF) |
| PDF 后端 | **krilla 0.8.2**(非 printpdf;printpdf 在本场景有一项实测致命缺陷,见 §3.1) |
| 布局引擎归属 | 新建 `latermd-render`(不 import egui,铁律 2),消费 latermd-md 的 token 流 |
| PDF 编码归属 | `latermd-export`(ADR-001 §5 / ADR-004 §5 既定:「HTML(P0)/ PDF / DOCX」) |
| 布局/分页 | 全部自建(CJK 断行、段落流、表格、代码块、分页、页码);无可用现成布局层 |
| B 路线复活条件 | 见 §5 |
| 新依赖锁定 | 见 §6 |

---

## 2. 背景与约束

### 2.1 需求来源

auto-plan #25「导出 PDF」:P0 只交付了导出 HTML,PDF 是导出线的第二形态。
任务书点名「headless 渲染(铁律 2 的验证场——latermd-render 不依赖 egui),printpdf 或
HTML→PDF 选型走 ADR;CJK 字体嵌入」。

### 2.2 三条硬约束

1. **铁律 2(本 ADR 的第一验收线)。** `latermd-render` 只定义布局/绘制指令结构,
   **不得 import `egui`**,由 `latermd-export` 把指令翻译为 PDF 调用。目的与
   「将来可做 headless CLI 导出器」的既有表述一致(AGENTS.md §3)。
   PDF 导出正是 roadmap「crate 增量创建表」写明的触发条件——**「出现第二个消费者时」
   创建 `latermd-render`**(roadmap.md:148:「render(绘制指令 IR)在只有 egui 一个
   后端时没有存在价值」)。PDF 导出就是那个第二消费者。
2. **铁律 1(单一解析器)。** PDF 路线的 Markdown 解析必须仍是 pulldown-cmark 0.13.4,
   扩展开关与 `latermd-export::parser_options()` 逐项一致
   (STRIKETHROUGH / TABLES / FOOTNOTES / TASKLISTS,latermd-export/src/lib.rs:95-102)。
   注意边界:路线 B 引入的 HTML 引擎解析的是**我们导出的 HTML 产物**,不是 Markdown,
   不构成第二个 Markdown 解析器;但它确实把「Markdown→可视形态」的渲染链路复制了一份(§3.2)。
3. **无签名分发的离线隐含要求。** 2026-09-24 定案的分发策略是 GitHub Release 直下 +
   Homebrew cask,**不买证书不做公证**(ADR-004 §3)。用户装完即用、无网络依赖是
   该策略的对价:PDF 导出若依赖「用户机器上装了 Chrome/Edge」或「首次导出时联网下载
   Chromium」,在离线机器与纯净系统上直接失效,并且不受我们测试矩阵控制——**出局**。

### 2.3 既有资产(两路线的起点差异)

- `latermd-export`:纯逻辑 crate,唯一依赖 pulldown-cmark,`export_html()` 产完整
  单文件 HTML(内嵌 CSS,系统字体栈)。**路线 B 的 body HTML 可 100% 复用**;
  路线 A 只复用其上游(latermd-md 的 token 流)。
- `latermd-md`:token/span 数据层已存在,是路线 A 的输入。
- `latermd-render`:**尚不存在**(ADR-004 §5 是终态图)。
- app 侧 CJK 字体候选表(fonts.rs:Win msyh.ttc/simhei.ttf/macOS PingFang.ttc/
  Linux NotoSansCJK.ttc + face 探测)——两路线的字体来源都可复用这张表。

---

## 3. 两路线逐维度证据

### 3.1 路线 A:纯 Rust 直绘(printpdf 系)

「printpdf 系」实际包含三个候选:printpdf本体、genpdf(其上的布局层)、krilla(同类定位的
现代替代)。**结论先行:选 krilla,printpdf 本体被实测数据否决,genpdf 已死。**

| 维度 | printpdf 0.12.8 | krilla 0.8.2 | 证据来源 |
|---|---|---|---|
| 许可 / MSRV | MIT / 1.88 | MIT OR Apache-2.0 / 1.92 | `cargo info`;均 ≤ 钉死的 1.98.0 |
| 依赖量(外部 crates) | **105**(default-features=false 实测) | **70**(实测) | /tmp 探针 `cargo metadata` 计数再减探针自身,§6.4 |
| 全新编译(release/debug) | 23.5s / 11.2s | **15.8s / 9.6s** | /tmp 探针 `cargo clean` 后计时 |
| release 探针二进制 | 2.4 MB | 4.6 MB(含默认 png/jpeg/gif/webp 编码器) | /tmp 探针 stat |
| TTF/TTC 嵌入 | ✓ `ParsedFont::from_bytes(bytes, font_index, …)`,官方示例注明「face index inside a .ttc collection」 | ✓ `Font::new(data, index)`,文档注明 index「for TrueType collections」(krilla 0.8.2 源码 font.rs:27-38) | 两库官方 README/源码;**两探针均实跑通过** |
| CJK 文字正确性 | ✓ pdftotext 回读逐字一致 | ✓ pdftotext 回读逐字一致 | /tmp 探针,系统 NotoSansCJK-Regular.ttc(19,484,784 B)face 2(SC) |
| **字体子集化** | **✗ 实测失效:`subset_fonts: true` 下输出 13,190,466 B(release 同值)≈ 整张 face 全量嵌入** | ✓ 自动子集化,同输入输出 **6,293 B**(3352 字体全量 → 6 KB,差距 2099×) | /tmp 探针实测,save 选项与官方 README 示例一致 |
| 布局/分页自理量 | 自己实现全部;XHTML 布局(feature `html`,azul-layout)官方自述「experimental / still evolving」,**未作推荐依据(未深测)** | 自己实现全部;README 明示布局/表格/分页「strictly out of scope」,定位「给有中间表示(IR)的库当后端」——与 latermd-render 的绘制指令 IR 设计**正对位** | 两库 README |
| 维护活跃度 | 0.12.8 发版 2026-09-05(近 6 周 4 版);repo pushed 2026-10-02,15 个未决 issue,1121 stars;144 万下载/90d | 0.8.2 发版 2026-06-04;repo pushed 2026-10-02,22 个未决 issue,450 stars;**150 万下载/90d**;作者 LaurenzV(typst 生态),建 on pdf-writer;90+ 快照测试 + 6 款阅读器视觉回归(README) | crates.io API + `gh api repos/…`,2026-10-04 |
| 底层 | lopdf 0.44 | pdf-writer(typst 系) | `cargo tree -i` 实测 / README |

**genpdf(printpdf 系的现成布局层)已死**:最后发版 0.2.0 = **2021-06-17,五年未发版**
(crates.io API),依赖钉在 `printpdf ^0.3.4` + rusttype + lopdf ^0.26(crates.io
dependencies API 实测)——与 printpdf 0.12.8 完全不兼容。「printpdf 系自带布局层」
这条路不存在,选 A 路线就是自建布局,后端选谁只看编码质量:**krilla 胜**(子集化实测、
测试基建、typst 系维护、依赖最轻)。

### 3.2 路线 B:HTML→PDF

**结论先行:唯一满足离线约束的引擎(fullbleed)太年轻且随包字体无 CJK(实测);
浏览器系引擎违反离线约束,直接出局。**

| 维度 | fullbleed 2.5.6(纯 Rust 引擎) | headless_chrome 1.0.22(Rust CDP 客户端) | wkhtmltopdf(外部引擎) |
|---|---|---|---|
| 形态 | crate,Rust API `FullBleed::builder().register_font_file(…).build()` + `compile_document(html, css)` + `render_to_buffer()`(源码实读) | crate,驱动**用户机上已装的** Chrome/Chromium(README:「control headless Chrome or Chromium over the DevTools Protocol」;`fetch` feature 联网下载 Chromium) | 外部二进制,需随包分发或要求用户安装 |
| 复用 latermd-export | body HTML 100%(仍是 pulldown-cmark 产物);CSS 需另写 print 变体(现 CSS 的 `@media (prefers-color-scheme)` 面向浏览器,引擎 CSS 子集未宣称支持 @media) | 100%(含暗色 @media) | 100%(Qt WebKit 引擎老,CSS 支持打折) |
| 离线可用 | ✓ 纯 Rust 无浏览器,README 明示「No headless browser requirement」 | **✗ 依赖用户装浏览器或联网下载** | △ 引擎可随包,但体积+许可负担 |
| 依赖量 / 编译 | **5 个外部 crates**(单体:73 个源文件、15,391 行 lib.rs);release 26.9s / debug 12.0s;release 探针二进制 9.6 MB | **142 个外部 crates**(实测 metadata 计数) | 不适用 |
| CJK 支持 | **随包字体零 CJK——实测不注册字体时中文全部变 `?`**(pdftotext 输出 `??,LaterMD ??`,栅格墨迹 0.31% 仅拉丁);注册系统 NotoSansCJK.ttc 后正常(墨迹 0.45%,27,078 B 子集化 OK);TTC face 按文件名启发式挑选(font.rs `preferred_collection_face_index`) | ✓ 浏览器字体栈 | ✓(引擎自带字体配置) |
| 分页/页眉页脚 | 引擎内置:`@page` size/margin、`Page {page} of {pages}` 页眉页脚、水印(README) | 浏览器 print CSS | 引擎参数 |
| 维护活跃度 | **crate 创建 2026-02-11(8 个月)**,总下载 2,519 / 90 天 2,181,repo 47 stars、0 未决 issue、pushed 2026-10-03(极活跃但社区极小);docs.rs 文档覆盖率 9.18%;版本 2.5.x 三天 6 版 | 1.0.22 发版 2026-06-11;**144 个未决 issue**;72 万下载/90d | **repo 已归档(archived:true),最后 push 2022-11-22,1352 未决 issue** |
| 许可 / MSRV | MIT / 1.85 | MIT | (未核实许可条款,已因归档出局) |

weasyprint(Python 外部引擎)按依赖形态同类出局(要求用户机装 Python 运行时),
未做深测,如实标注。

### 3.3 汇总对比(A-krilla vs B-fullbleed,两路线各自最优代表)

| 维度 | A:krilla 直绘 | B:fullbleed HTML→PDF |
|---|---|---|
| 铁律 2 | ✓ **正是 latermd-render 的创建触发点与验证场** | △ 不经过 latermd-render(HTML 直达引擎),铁律 2 无从验证 |
| 铁律 1 | ✓ pulldown-cmark token 流直消费 | ✓ Markdown 仍只 pulldown-cmark(HTML 引擎非 Markdown 解析器);但渲染链路成了第二份 |
| 排版一致性 | △ 第三套渲染实现(预览 egui / HTML 导出 CSS / PDF 自建),与预览的观感漂移要自己钉 | ✓ 与 HTML 导出同源,CSS 稍作 print 变体即近乎一致 |
| 离线/分发 | ✓ 纯 Rust,零外部运行时 | ✓ 纯 Rust(但 CJK 依赖运行时注册系统字体,Win/mac 候选表复用 fonts.rs) |
| CJK+子集化 | ✓✓ 实测 6 KB | ✓ 实测 27 KB(需显式注册字体,否则豆腐) |
| 布局工作量 | **全部自建**(CJK 断行/段落流/表格/代码块/分页/页码) | 接近零(引擎内建) |
| 依赖/编译 | 70 crates / 15.8s / 4.6 MB | 5 crates / 26.9s / 9.6 MB |
| 成熟度 | 高(typst 系,150 万下载/90d) | **低(8 个月,2.5 千总下载,47 stars,文档率 9%)** |
| 版本风险 | 0.x(见 §6 锁定策略) | 2.5.x 三天 6 版,API 未标稳定,docs.rs 文档率 9.18% |

---

## 4. 推荐路线与理由

**推荐:A 路线,PDF 后端 krilla 0.8.2。**

1. **铁律 2 是本功能的使命而非负担。** roadmap 把 latermd-render 的创建条件写死为
   「出现第二个消费者时」,PDF 就是第二消费者。走 B 路线则 latermd-render 依旧不存在,
   「headless 导出验证场」落空,铁律 2 永远停留在纸面。
2. **实测质量差距是数量级的。** 同一张 19.5 MB 系统 TTC、同一段中英混排:krilla 6,293 B、
   printpdf 13,190,466 B(子集化实测失效)。printpdf 的 CJK 嵌入在「导出一份能邮件发出
   去的 PDF」场景直接不可用。
3. **离线约束砍掉 B 路线的大多数引擎。** headless_chrome(要求用户装 Chrome,142 crates)、
   wkhtmltopdf(已归档)出局后,B 只剩 fullbleed——8 个月大、总下载 2.5 千、文档率 9%、
   随包字体无 CJK(实测)。把导出这样的一次性核心功能押在它上,风险与「小表面积」的
   工程约定相悖。
4. **布局自建的成本可控且有既定归属。** 布局本来就要落在 latermd-render(绘制指令 IR),
   与预览共用 latermd-md 的 token 流;需要自建的清单:CJK 换行(Unicode line breaking)、
   文本测量、段落/标题/列表/引用/表格/代码块布局、分页与页码。这是 #43(预览排版)
   已趟过的字体度量领域的延伸,不是从零发明排版学。**代价如实写明:这是本路线最大的
   成本项,B 路线在「布局工作量」上确实近乎为零。**
5. **版本与许可干净。** krilla MIT OR Apache-2.0(仓库 MIT),MSRV 1.92 ≤ 钉死的
   1.98.0,70 crates 全新增编译面里最轻,typst 生态维护。

**被否路线的明确否决理由归档**:printpdf(子集化实测失效)/ genpdf(2021 年停更且钉
printpdf 0.3 线)/ headless_chrome(离线约束)/ wkhtmltopdf(归档)/ weasyprint(外部
Python 运行时)。

---

## 5. 被否路线(B:HTML→PDF)的复活条件

同时满足以下条件时,可在后续 ADR 重开 B 路线(届时 A 路线的布局引擎若已建成,
B 仍需论证存在价值):

1. **fullbleed 成熟度**:crate ≥12 个月历史且总下载量上到 10 万量级,或发布 1.0/稳定
   API 承诺;docs.rs 文档率不再是 9% 量级。
2. **CJK 一等公民**:随包字体覆盖 CJK,或「注册系统字体」进官方文档与测试矩阵
   (Win msyh.ttc / mac PingFang.ttc 实测通过)。
3. **CSS 覆盖对齐**:官方「CSS coverage and remaining gaps」文档证明
   表格/引用/代码块/任务列表与浏览器渲染无回归。
4. 或出现另一款纯 Rust、离线、CJK 就绪的 HTML→PDF 引擎满足 1-3。

另有**功能内检查点**:A 路线实施中,若「CJK 断行 + 段落流 + 表格」三个里程碑中任一
在两个工作日内无法达到可演示状态,立即回看本 ADR 与 #96,由用户裁决是否切 B。

---

## 6. 新依赖的版本锁定要求

按 decisions-pending #4 口径,**代码落地时**同步登记 ADR-004 §2(版本+用途+notes);
本 ADR 是草案,不预先改动 ADR-004 与 docs/README.md 索引(追认后随首个实现 commit 一起进)。

| 依赖 | 锁定 | 用途 | notes |
|---|---|---|---|
| `krilla` | **0.8.2**(Cargo.toml 写 `0.8.2`,Cargo.lock 钉死;0.x 的 minor 即破坏性版本,升级须过 PDF 快照回归) | PDF 编码后端(latermd-export 依赖,不进 app) | 默认 features(raster-images+simple-text)即可:图片四格式编码器与图片框白名单对齐;rustybuzz 随 simple-text 进来,CJK 无需复杂 shaping 亦可正确落字;铁律 2——latermd-render 不 import egui,krilla 也不依赖 egui(70 个外部 crates 实测树内无 egui) |
| (候选)`unicode-linebreak` 或等价 | 实现模块定版后登记 | CJK/西文统一断行 | 若实现模块选择自写断行(仅 CJK 字符间可断+西文按空格),则不引入 |
| `latermd-render` | 新 crate(path 依赖) | token→布局指令 IR,不 import egui | 创建时机正是本功能(roadmap「出现第二个消费者时」);**cargo tree 取证无 egui 是实现模块的验收线** |

其余约束:

- 解析开关:`latermd-render`/`latermd-export` 的 PDF 链路与 `parser_options()`
  逐项一致(铁律 1);实现模块需带一致性单测(照 latermd-export 既有测试形态)。
- MSRV:krilla 1.92;全 workspace 钉 1.98.0 不变。
- 三平台字体:实现时复用 fonts.rs 候选表;**Win msyh.ttc / mac PingFang.ttc 的
  face index 与子集化必须三平台实测**(本 ADR 只在本机 Linux NotoSansCJK 实测,
  PingFang.ttc face index 沿 M0 挂账未核验项)。

### 6.4 实测复现方法(证据可重放)

```
# 探针(2026-10-04,/tmp 下四个独立工程,rustc 1.98.0)
cargo info printpdf; cargo info krilla; cargo info fullbleed; cargo info genpdf
# 依赖计数/编译计时:每工程 cargo clean 后分别 time cargo build [--release]
# CJK 探针:读系统 NotoSansCJK-Regular.ttc(face 2=SC)→写中英混排一页→落盘
#   krilla:   Font::new(bytes,2)+surface.draw_text → pdftotext 回读一致,6,293 B
#   printpdf: ParsedFont::from_bytes(&bytes,2,…)+Op::ShowText+subset_fonts:true
#             → pdftotext 回读一致,但 13,190,466 B(debug/release 同值)
#   fullbleed: builder+compile_document(html,css)+render_to_buffer,nofont/withfont
#             双模式:前者 pdftotext 全 `?`、墨迹 0.31%;后者回读一致、27,078 B、0.45%
# 维护度:crates.io API(/api/v1/crates/<name> 与 /versions)+ gh api repos/<owner>/<repo>
```

---

## 修订记录

| 日期 | 变更 |
|---|---|
| 2026-10-04 | 初版(草案):A/B 两路线调研,五候选四维度实测,推荐 A+krilla 0.8.2;登记 decisions-pending #96 待用户追认 |
