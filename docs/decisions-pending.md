# 待用户决策记录（decisions-pending）

> 自动开发循环遇到「本该问用户」的岔路口时，在这里登记：**岔路是什么、自动选了什么、为什么、想改怎么改**。
> 选择由循环自行做出并继续执行，不阻塞；用户事后翻此文件，按「如何改」一节操作即可推翻。
> #30 曾是「等待型」条目（改窗口形态本身，返工成本高），2026-09-26 用户放行后已按默认全部落地（M1–M4 合入 main）。
> 编号 #98 为当前最新条目。

## #98 #51 M3 app 侧 mermaid widget 四处口径:超宽图整图等比缩进面板而非横向留白、Live 列用只拦 mermaid 的独立 handler 而非复用 AiLinkHandler、块序号取本帧文档序计数、回落用「嵌套无 handler 的 MarkdownLabel」而非手写代码块(2026-10-04,#51 mermaid-render M3·自动拍板)

- **岔路**:①任务书「widget 高度=布局高度,宽度超面板时按比例缩小或水平留白,取舍写 notes」二选一留给实现;②Live 富渲染块接 mermaid 的方式:复用右栏预览的 `AiLinkHandler`(一并带上 ai 卡/emoji/wikilink 行为)还是新造只拦 mermaid 的 handler;③任务书「用块序号+内容哈希等稳定键」——vendored `block_code_widget` 回调不带 token 序号,块序号从哪来;④回落态源码代码块的画法:vendor 的 `render_code_block` 是私有函数,app 侧手写近似代码块还是嵌套 `MarkdownLabel`。
- **备选**:①a 等比缩放(字号随之缩小) ①b 原尺寸+水平溢出留白/裁切;②a 复用 AiLinkHandler ②b 独立 LiveMermaidHandler;③a vendor ①类补丁透传块序号 ③b app 侧 handler 帧内计数(照 ```ai 卡 `card_count` 模式);④a 手写代码块(背景+高亮+按钮) ④b 嵌套 MarkdownLabel 渲染原围栏文本。
- **自动选择**:①等比缩放,`scale = min(1, 面板宽/图宽)`,缩放同步作用于全部坐标与文字字号,窄于面板时水平居中留白;②独立 `LiveMermaidHandler`(只实现 `is_block_code_widget`/`block_code_widget` 对 mermaid 命中,其余全走 vendored 默认);③handler 帧内计数器(preview 侧进 `AiLinkHandler::mermaid_count`,与 `card_count` 分开计数;widget/缓存键 = 外层 label id + 块序号,内容只进缓存条目的 content_hash);④嵌套 `MarkdownLabel::new(稳定id, "```mermaid\n{源}\n```")`,**不传 link_handler**。
- **理由**:①流程图超宽时留白等于右侧内容永远不可见,缩放保完整性;布局缓存不含面板宽,拖窗宽不重排只重缩放,流式期间也不因面板抖动清缓存;字号同比缩小使「文字在盒内」的几何关系在任意 scale 下近似保持;②Live 列此前不接任何 handler,复用 AiLinkHandler 会顺带改变 ai 卡/emoji/wikilink 在 Live 的渲染行为,越出 #51 的改动边界(否决线:非 mermaid 块路径不变);③vendor 补丁透传序号要动 ①类补丁+登记,而帧内计数在「文档结构不变时每帧序号稳定」上与 token 序号等价,流式追加只动最后一块,上游插入 mermaid 块才平移下游序号(且只清 mermaid 自己的缓存);④手写代码块要么放弃 syntect 高亮要么给 app 新增 syntect 直接依赖,嵌套 label 零 vendor 改动复用 vendored 高亮+复制按钮全链路,不传 handler 即 `is_block_code_widget` 恒 false,天然不可能递归回 mermaid widget。
- **如何改**:①要改为留白:`ui/mermaid.rs` `paint_diagram` 的 `scale` 固定 1.0、水平居中改左对齐,超宽部分交给面板横向滚动(需另开横向 ScrollArea);②要让 Live 也渲染 ai 卡/emoji:Live 列改为构造 `AiLinkHandler`(需传 `AiState`,live.rs 现无此参数,要改 `live::ui` 签名并从 App 层传入);③要 token 序号:vendor label.rs `block_code_widget` 调用点(约 951 行)加 `push_id(i)` 包裹或改 trait 签名透传(①类补丁,vendor/README 登记);④要手写:在 `paint_fallback` 里用 `ui.visuals().code_bg_color` 画底+`highlight_code` 同款逻辑(app 需直接依赖 syntect,先在 ADR-004 登记依赖再动)。

## #97 #51 M1 mermaid 解析与布局的六处口径:渲染路线取纯 Rust、方向仅收 TD|LR 不收 TB 别名、`---` 按书写方向参与分层、孤立单节点 Ok 不 Err、跨层长边拆虚拟节点走正交肘形、链式边/引号标签收编而更长横线与 `&` 并列显式 Err(2026-10-04,#51 mermaid-render M1·自动拍板)

- **岔路**:①渲染路线(auto-plan #51 已拍板「纯 Rust 内嵌 vs 外部 mmdc(node)/远程 kroki——取纯 Rust」,五要素需在本表在册);②方向别名:任务书钉死「flowchart/graph TD|LR」,但 mermaid 另有 TB(TD 的别名)/BT/RL 与小写写法;③`---` 无向边在分层布局里的方向语义;④任务书「空图/孤立单节点/环(a-->b-->a)不 panic 且 Err 有可读信息」一句中,孤立单节点到底是 Ok 渲染还是 Err;⑤连线形态:任务书允许「折线或正交连线」二选一,跨层长边(A→C 中间隔着 B 层)是否上 Sugiyama 虚拟节点;⑥标签边界:双引号标签、链式 `A --> B --> C`、更长横线 `---->`、`-- 文字 -->`、`A & B` 并列是否算 v1 子集。
- **备选**:①外部 mmdc / 远程 kroki;②a 顺手收 TB 等别名 ②b 严格只收 TD|LR;③a 无向边双向等价 ③b 按书写方向定向参与分层;④a 孤立单节点 Err ④b Ok 渲染一个盒;⑤a 直线折线 ⑤b 正交肘形+跨层长边拆虚拟节点 ⑤c 正交肘形不拆虚拟节点(垂直段可能穿中间层盒子,靠「宽松断言」兜);⑥a 严格最小集(链式/引号也 Err)⑥b 收编链式与引号、其余 Err。
- **自动选择**:①纯 Rust 内嵌(latermd-render `mermaid` 模块,零新增 crates.io 依赖);②严格 TD|LR,TB/BT/RL/小写一律 Err(错误信息写明「仅支持 TD|LR」);③`---` 按书写方向参与分层与消交叉,绘制时不带箭头;④孤立单节点 Ok 渲染一个盒,空图(0 节点)与环 Err 且消息可读(环给出 `a → b → a` 全路径);⑤**b**:正交肘形折线 + 跨层长边拆零尺寸虚拟节点(参与 barycenter 排序与列槽占位),因此全部边只走相邻层,折线不穿任何盒——任务书的「宽松断言」被升级成严格断言(样本图含跨层长边也零穿越);⑥b:链式边拆成多条边、双引号标签收编(容纳 `)`/`<` 等特殊字符),`---->`/`-- 文字 -->`/`A & B`/`A((x))` 复合形状等显式 Err。
- **理由**:①离线+隐私+铁律 2(auto-plan 已拍板,此处落档五要素);②别名集没有任务书背书,「不猜测不吞错」宁可 Err 回落为源码代码块也不静默猜方向,TB 是高频写法但收编它是扩子集的决定,应显式做;③无向边若按双向等价处理,任何 `A --- B` 都成环、分层不变式被破坏,按书写方向定向是 dagre 系通行做法,视觉上无箭头仍忠实「无向」原义;④mermaid 本身渲染孤立节点,Err 会把合法输入错打成回落代码块;任务书该句的可辨认意图是鲁棒性(不 panic),空图与环才真正无法产出图;⑤首版实现先走了 c,样本图(判断分支 B→D 跨层)立刻暴露垂直段穿中间层盒——分支汇合是 flowchart 最常见形态,靠挑样本图绕过等于把缺陷留给用户文档,虚拟节点是 Sugiyama 标准第二步、约 40 行,换来「任意图零穿盒」的强保证;⑥链式与引号是 flowchart 核心语法、真实文档高频,收编成本十数行;更长横线/并列/复合形状超出任务书枚举,显式 Err 保持子集可审计(v1 子集边界由测试逐条钉住)。
- **如何改**:①要切外部引擎(mmdc/kroki)——推翻 auto-plan #51 的拍板本身:删 latermd-render 的 mermaid 模块与 app 侧 `ui/mermaid.rs`,`block_code_widget` 分支改为起外部进程/发远程请求取图;须先接受 mmdc 依赖 node 运行时、kroki 需联网(离线+隐私两条理由失守),且铁律 2 的 headless 渲染场随之外包给外部工具链;②要收 TB:改 `crates/latermd-render/src/mermaid/parser.rs` 方向 match,加 `"TB" => Direction::TopDown` 分支(小写别名同理);③要无向语义:`layout` 的环检测与分层前把 `---` 边从邻接表剔除(分层不再受它约束,绘制仍画线);④要 Err:在 `parse` 收尾处对 `nodes.len()==1 && edges.is_empty()` 加错误分支;⑤要退回直线折线:删 `layout.rs` 的虚拟节点拆链段(`chains`/`ext_layers`),`elbow` 换成两点直连,并把 mod.rs 的 `edge_polylines_avoid_boxes` 断言降回「相邻层边」子集;⑥要放行 `---->`:在 `parse_statement` 的箭头 token match 加更长横线模式;要收 `A & B`:在 `node_ref` 前加 `&` 分隔的多节点展开。M2/M3 若发现回落率过高,优先按本条扩子集而不是放宽「不吞错」纪律。

## #96 #25 PDF 导出技术路线:A 纯 Rust 直绘(krilla 后端)而非 B HTML→PDF,printpdf 被实测否决(2026-10-04,#25 export-pdf M1·自动拍板)

- **岔路**:PDF 导出只有两族技术路线——A「纯 Rust 直绘」(Markdown token → 自建布局引擎 → PDF 编码后端,后端候选 printpdf / genpdf / krilla)与 B「HTML→PDF」(复用 latermd-export 的 HTML 产物,引擎候选纯 Rust 的 fullbleed、Rust CDP 的 headless_chrome、外部的 wkhtmltopdf/weasyprint)。这也是 roadmap「crate 增量创建表」里 `latermd-render`(「出现第二个消费者时」创建)是否随 PDF 落地的岔路:走 A 则 latermd-render 本次创建并成为铁律 2 验证场,走 B 则 HTML 直达引擎、latermd-render 依旧不创建。选型证据全文见 [adr-006-export-pdf-route.md](adr-006-export-pdf-route.md)(草案,待追认)。
- **备选**:A1 printpdf 0.12.8;A2 krilla 0.8.2;A3 genpdf(printpdf 系布局层);B1 fullbleed 2.5.6(纯 Rust HTML/CSS→PDF);B2 headless_chrome(用户机浏览器);B3 wkhtmltopdf/weasyprint(外部引擎)。
- **自动选择**:**A2 = krilla 0.8.2**。新建 `latermd-render`(token→布局指令 IR,不 import egui)承担 CJK 断行/段落流/表格/代码块/分页/页码的全部布局,`latermd-export` 依赖 krilla 0.8.2 做编码后端;后续模块(编排驱动)按 [adr-006-export-pdf-route.md](adr-006-export-pdf-route.md) §6 锁定实现。
- **理由**:①离线约束先砍人——无签名分发(ADR-004 §3)的对价是装完即用,headless_chrome 要求用户装 Chrome(README 明示驱动本机 Chrome/Chromium)、wkhtmltopdf 仓库已归档(gh api 实测 archived:true、2022-11-22 停推),B 只剩 fullbleed;②fullbleed 不够格押注——crate 2026-02-11 才创建(8 个月)、总下载 2,519、47 stars、docs.rs 文档率 9.18%,且**随包字体零 CJK(实测:不注册字体时中文全部渲染为 `?`,pdftotext 输出 `??,LaterMD ??`)**,必须运行时注册系统字体才行;③A 路线后端里 printpdf 有实测致命伤——同一张 19.5MB 系统 TTC、同一段中英混排、`subset_fonts: true`,printpdf 输出 13,190,466 B(子集化失效,debug/release 同值)而 krilla 6,293 B,差 2099 倍,「能邮件发出去的 PDF」场景直接不可用;genpdf(printpdf 系现成布局层)最后发版停在 2021-06-17 且钉 printpdf ^0.3.4 旧线,不可用;④krilla 其余维度全面占优或持平——70 个外部 crates(探针实测;printpdf default-features=false 也有 105)/release 全新编译 15.8s/维护极活跃(0.8.2 于 2026-06-04,150 万下载/90d,作者 typst 生态 LaurenzV,90+ 快照测试+6 款阅读器视觉回归)/MIT OR Apache-2.0/MSRV 1.92≤钉死的 1.98.0/TTC face index 与 CJK 落字探针实跑通过(pdftotext 回读逐字一致)/cargo tree 实测树内无 egui;⑤铁律 2 的验证场是本功能的使命——roadmap 写明 latermd-render「出现第二个消费者时」创建,PDF 就是第二消费者,走 B 则铁律 2 永远停留在纸面。**A 路线的代价如实承认:布局全部自建(B 路线的 fullbleed 引擎内建 @page/页眉页脚,布局工作量近乎为零),这是本选择最大的成本项,ADR §5 已设「CJK 断行+段落流+表格任一里程碑两工作日不可演示即回看」的检查点。**
- **如何改**:要切到 B 路线(fullbleed)——用户在本文档或 adr-006 上批注追认 B,按 adr-006 §5 复活条件核对(fullbleed 成熟度/CJK 一等公民/CSS 覆盖对齐),把 adr-006 改版为 B 决议(后端 fullbleed 2.5.6、运行时注册 fonts.rs 候选表字体、CSS 写 print 变体),latermd-render 创建推迟;要留 A 但换 printpdf 后端——需先解决其 TTC 子集化 13MB 实测问题(上游 issue 跟踪或改嵌独立 TTF),否则维持 krilla;要改 krilla 版本/特性——改 adr-006 §6 表(实现模块落地时按 #4 口径同步登记 ADR-004 §2 与 docs/README.md 索引,本草案未动这两个文件)。

## #95 M3 设置页三选一的四处落地口径:切 provider 出厂值跟随(手改不动)、空端点按当前 provider 回落、connects_network 与 requires_key 解耦、旧拼法 openai_compatible 加 alias(2026-10-04,#20 ai-adapters M3·自动拍板)

- **岔路**:①设置页把 provider 从 OpenAI 兼容切到 Anthropic/Ollama 后,表单里的端点/模型/超时怎么办——原样保留(切完保存即坏配置:provider=Ollama 端点还是 api.openai.com)、无条件重置为新家出厂值(用户为前一家手改的网关地址被静默冲掉)、还是仅当当前值仍是「任一 provider 出厂值」(即用户从未手改)时才跟随;②`normalize()` 的空端点/模型回落——旧版回落全局默认(OpenAI 出厂值),对 provider=Ollama 的手改 JSON 回落 api.openai.com 是错的;③`connects_network()` 旧实现 = `requires_key()`,Ollama 无 key 也连本机 11434,两维是否解耦;④旧 `ai.json` 的 provider 字段:serde snake_case 把 `OpenAiCompatible` 落成 `open_ai_compatible`,而任务书写的是 `openai_compatible`(无下划线),按哪种拼法保兼容。
- **备选**:①a 原样保留;①b 无条件重置;①c 出厂值跟随(手改不动)。②a 全局默认;②b 当前 provider 出厂值。③a 维持 =requires_key;③b 解耦为 =uses_settings()(唯 Mock 不联网)。④a 只认旧落盘名 `open_ai_compatible`;④b 再加 `#[serde(alias = "openai_compatible")]` 两种拼法都收。
- **自动选择**:①c + ②b + ③b + ④b。`AiConfig::adopt_provider_defaults(new)`:端点/模型/超时若等于任一 provider 的出厂值则换成 new 的出厂值(含 Ollama 超时 120 本地推理档),手改过的字段原样保留,绝不静默覆盖;`normalize()` 空端点/模型回落**当前 provider** 的出厂值;`connects_network()` 改为 `provider.uses_settings()`(Mock 唯一 false,设置页端点行对 Ollama 也显示);`OpenAiCompatible` 变体加 alias,`open_ai_compatible`(旧版真实落盘名)与 `openai_compatible`(任务书拼法)都能读,落盘仍写 `open_ai_compatible`。出厂值单一事实源:`ProviderKind::factory()` 直接取 latermd-ai 各 adapter 的 `Default` 实现,app 侧不另抄常数。
- **理由**:①c 是「不丢用户输入」与「不产出坏配置」的交集:出厂值跟随覆盖了全新安装与从未手改的主流路径,手改值可见可改,静默覆盖是配置页最伤信任的行为;②provider 换了,「空值该回到哪」的语义自然跟着换,回落 OpenAI 地址对新 provider 是二次伤害;③Ollama 的端点是真实配置项(默认 http://127.0.0.1:11434),端点行不显示会让用户以为没生效;requires_key 驱动命令闸门、connects_network 驱动端点显示,本就是两个问题;④alias 只影响反序列化,零成本兜住两种拼法,向后兼容对实际旧文件(`open_ai_compatible`)与任务书口径(`openai_compatible`)同时成立。
- **如何改**:①要无条件重置,把 `adopt_provider_defaults` 里三段 `factories.iter().any(...)` 判断删掉直接赋值(接受手改值被冲掉);②把 `normalize()` 里两处 `factory.base_url`/`factory.model` 换回 `Self::default().base_url`/`model`;③把 `connects_network()` 改回 `self.provider.requires_key()`(Ollama 端点行随之不显示);④删变体上的 `#[serde(alias = "openai_compatible")]` 并同步删 `legacy_ai_json_with_api_style_loads` 里无下划线拼法的断言分支。

## #94 M3 provider 与 ApiStyle 的关系收敛:删除 ApiStyle 字段,ProviderKind 成为唯一开关(2026-10-04,#20 ai-adapters M3·自动拍板)

- **岔路**:任务书要求「优先让 ProviderKind 成为唯一开关、api_style 随 provider 派生」,并授权按小表面积原则取舍「保留或删除 ApiStyle 字段」。三条路:①保留 `AiConfig.api_style` 字段 + `normalize()` 派生覆盖(每次读写都把派生值写回去);②保留字段但只作展示、保存时强制对齐;③整字段删除,协议形态信息移进 provider 的 label/description,UI 的「接口方式」下拉随之退役。
- **备选**:①字段 + 派生回写;②字段仅展示;③删除字段。
- **自动选择**:③。`ApiStyle` 枚举、`AiConfig.api_style` 字段、`normalize()` 的「未实现回落 ChatCompletions」段、设置页「接口方式」下拉与「尚未实现」警示行全部删除;provider 下拉显示名改为带协议形态(「OpenAI 兼容(/chat/completions)」「Anthropic(/v1/messages)」「Ollama 本地(/api/chat)」,Ollama 从任务书点名的错误标签 `/api/generate(未实现)` 更正为实际实现的 `/api/chat`),新增 `ProviderKind::description()` 说明行承担 key 需求与端点性质提示。旧 `ai.json` 里遗留的 `api_style` 键读取时被 serde 忽略(结构体未开 `deny_unknown_fields`),向后兼容不受影响。
- **理由**:四种 provider 各自钉死一种协议后,`api_style` 是 100% 可从 `provider` 派生的冗余状态——保留它,手改 JSON 就能造出「provider=anthropic + api_style=chat_completions」的矛盾组合,归一化与 UI 都要为「对齐派生值」写防御代码,表面积不减反增;它还是「未实现」误导文案(Anthropic /v1/messages 未实现、Ollama /api/generate 未实现)的载体,M1/M2 落地后这些状态已不存在,连字段一起删才删得干净。删除后配置文件少一个字段、UI 少一个下拉、normalize 少一条回落规则,符合任务的小表面积授权。
- **如何改**:要恢复「同一 provider 多协议」的将来形态(如 OpenAI 兼容端点同时支持 Responses API),把 `ApiStyle` 枚举与字段加回 `ai_config.rs`(serde snake_case),`set_provider` 分派处按 (provider, api_style) 二元组 match,设置页恢复第二下拉;git 历史里本条目的删除 commit 即完整反参照。

## #93 M2 Ollama 适配器的四处口径:超时默认 120(非范本的 60)、num_predict 取 i32(负值合法)、error 行文案带「Ollama 流失败」前缀、complete_sync 恒非流式照 anthropic 口径(2026-10-04,#20 ai-adapters M2·自动拍板)

- **岔路**:①`timeout_secs` 默认值任务未规定,openai/anthropic 范本共用 60,但 Ollama 是本地推理——冷启动先要把几 GB 模型载入内存(CPU 机器数十秒);②`num_predict`(= 各家 max_tokens)类型未规定:openai 的 max_tokens 是 `Option<u32>`,而 Ollama 的 num_predict 负值是协议合法取值(-1 不限、-2 填满上下文窗口);③NDJSON 顶层 error 行的失败块 delta 文案:裸服务端字符串还是带前缀(M1 #92 ③b 同款问题);④`complete_sync` 的请求体:照 openai.rs 原样塞设置里的 stream(流式配置下会拿到 NDJSON 却按单 JSON 解析),还是照 anthropic.rs 恒按 stream=false 请求。
- **备选**:①a 60(与范本一致);①b 120(本地推理加倍)。②a `Option<u32>`(与 openai max_tokens 同型);②b `Option<i32>`。③a 裸字符串;③b 前缀+字符串。④a 照 openai(设置原样透传);④b 照 anthropic(恒非流式)。
- **自动选择**:①b + ②b + ③b + ④b。timeout 默认 120;num_predict 为 `Option<i32>`(负值原样透传,单测钉了 -1);error 块 delta 为 `Ollama 流失败:{error}`(error 为 null 的行不当错误、非字符串退「未知错误」);`request_body(prompt, stream)` 带 stream 参数,complete_sync 恒传 false。
- **理由**:①60s 对本地冷启动是误伤,用户首次点「续写」大概率撞超时报「AI 请求失败」,不是用户可修的问题;本地慢是常态而非异常,加倍是最小偏离。②u32 表达不了 Ollama 老手常用的 -1;serde 对 i32 透传无成本,配置页将来加下界校验即可。③Chunk 失败约定要求 delta 面向用户,裸 "model 'x' not found, try pulling it first" 在 UI 上无来源上下文;前缀还与 #92 ③b、openai 的「AI 请求失败:{err}」同构,用户能区分错误来自哪家。④openai.rs 那套是被 anthropic.rs 注释点名过的既有瑕疵(流式配置下 complete_sync 必炸),M1 已改对,Ollama 跟随。
- **如何改**:①改 `crates/latermd-ai/src/ollama.rs` `OllamaSettings::default()` 的 timeout_secs(单测 `default_settings_match_documented_defaults` 同步);②类型改 `Option<u32>` 并删「负值透传」单测断言;③改 `error_chunk` 的 format 前缀(单测 `ndjson_error_line_yields_failed_chunk_and_stops` 同步);④把 `request_body` 的 stream 参数删掉改读 `self.settings.stream`(不建议,见 openai.rs 同问题的既有行为)。

## #92 M1 Anthropic 适配器的三处口径:默认型号取 Sonnet 系别名、采样参数默认 None 不发、错误事件文案带「Anthropic 流失败」前缀(2026-10-04,#20 ai-adapters M1·自动拍板)

- **岔路**:①任务要求「model 默认取 Anthropic 当前主力型号,注释说明来源」,但官方 docs.anthropic.com 对本地区域封锁(实测返回 supported-countries 拦截页),取不到官方 model 表;第三方信息混乱(2026-02 有 Claude Opus 4.6 发布报道但均非官方来源,另有未证实的「Claude Mythos」泄露帖),且「主力」有两解——能力旗舰(Opus 级)还是多数任务的默认推荐(Sonnet 级);②temperature/top_p 的默认值未规定:镜像 openai.rs 的 Some(0.7)/Some(1.0),还是 None 不发;③SSE error 事件的失败块 delta 文案格式未规定:裸服务端 message 还是带前缀。
- **备选**:①a `claude-sonnet-4-5`(Sonnet 系 alias);①b `claude-opus-4-6`(能力旗舰,存在性仅非官方报道);②a 默认 None/None;②b 镜像 openai 默认 0.7/1.0;③a 裸 message;③b 前缀+message。
- **自动选择**:①a + ②a + ③b。model 默认 `claude-sonnet-4-5`(alias 自动跟随官方最新快照,不钉日期版本,来源与查证受限事实已写进 anthropic.rs 的字段注释);temperature/top_p 默认 None(不发字段=用官方默认 temperature=1.0);error 块 delta 为 `Anthropic 流失败:{message}`(message 缺失退 error.type,再退「未知错误」)。
- **理由**:①Anthropic 文档口径是 Sonnet 级为多数任务的默认推荐、Opus 留给最难任务;openai.rs 范本默认同为入门级(gpt-4o-mini),「主力」按默认推荐取;Opus 单价比 Sonnet 高数倍,LaterMD 的续写/commit/摘要场景不是 Opus 级任务;alias 形式不受快照迭代失效影响。②官方端点默认值明确,「不发」语义最干净;openai.rs 硬塞 0.7 的理由是「各家兼容端点默认值不一致」,对官方 Anthropic 端点不成立。③Chunk 失败约定要求 delta 是面向用户的错误描述,裸「Overloaded」在 UI 上无上下文,前缀还让用户能区分错误来源(与 openai.rs 的「AI 请求失败:{err}」同构)。
- **如何改**:①改 `crates/latermd-ai/src/anthropic.rs` 里 `AnthropicSettings::default()` 的 model 字段(单测 `default_settings_match_documented_defaults`、`request_body_is_anthropic_messages_stream` 的断言同步改);②同文件 default 的 temperature/top_p 改 `Some(0.7)`/`Some(1.0)` 并同步上述单测;③改 `error_chunk` 的 format 前缀(单测 `sse_error_event_yields_failed_chunk_and_stops` 同步)。

## #91 #48 B2 emoji inline widget 的两处口径:标题里的 emoji 不随标题字号缩放(沿用 vendored 链接基础字体)、手型抑制在 app 侧渲染后压回(不改 vendor)(2026-10-04,#48 emoji-color B2·自动拍板)

- **岔路**:①可行性调查 §3.2 点名要验的「heading 段是否吃到 inline widget」实测落地:吃到了(图片落在标题行内),但 vendored 层对 `Token::Link` 一律传**正文基础字体**(layout.rs `Token::Link` 分支不走 `Token::Text` 的 heading 字号放大),`inline_widget_size`/`layout_link` 拿到的 font 恒为正文档 —— 标题里的 emoji 彩图因此不随 H1-H6 缩放;②vendored 对悬停中的 inline widget **无条件**置 PointingHand(label.rs `handle_hover` 的 inline-widget 分支、layout.rs `render_link_in_ui`),`link_style` 管不到光标,任务书却要求「不吃手型」。
- **备选**:①a 接受(emoji 在标题里按正文字号画,与 wiki:///ai:// 链接文本在标题里的既有行为同源);①b vendor ①类补丁:`Token::Link` 分支继承当前 heading 的放大字体(或引入 current font 上下文),登记 vendor/README 变更表;②a app 侧在 `MarkdownLabel::show` 之后按本帧 emoji widget 区块把悬停光标压回 `CursorIcon::Default`(同面板内后写者胜);②b vendor 补丁给 `LinkStyle` 加 cursor 字段或 inline widget 关手型开关。
- **自动选择**:①a + ②a。标题覆盖断言(preview.rs `emoji_inline_widget_paints_inside_heading_at_link_font_scale`)把两个事实钉成已知如实行为:图片纵向落在标题行带内(生效)、边长与段落档一致(不缩放)。
- **理由**:B2 任务红线「app 侧实现,不改 vendor」直接排除 ①b/②b;①b 还会牵动**所有**链接(含 http/ai://wiki://)在标题里的字形 metrics,影响面远超 emoji;现状并非 LaterMD 缺陷而是上游简化(标题里的任何链接文本都按正文渲染,emoji 只是首次把这个差异**画**出来);②a 的副作用面已收敛:压回只发生在指针落在本帧 emoji 区块内、且写点在 label 渲染之后、同帧后续面板(编辑器/侧栏)之前 —— 后续面板按各自悬停自设光标,不受影响(无头双向断言:emoji 上 Default、普通链接上 PointingHand 均钉住)。
- **如何改**:①要 emoji 随标题缩放 —— vendor/egui_markdown/src/layout.rs `Token::Link` 分支把 `font_id` 换成继承当前 heading 放大后的 format(照 `Token::Text` 的 heading 分支同款推导),按 §6 ①类登记 vendor/README 变更表并跑 check.sh,app 侧零改动(`inline_widget_size`/占位宽度都按传入 font 派生,自动跟随);②要更根治的手型口径 —— vendored `LinkStyle` 加 `cursor: Option<CursorIcon>` 字段(①类),app 侧删掉 preview::ui 里 label 渲染后的压回段与 `emoji_rects` 探针的抑制用途(探针保留供无头断言)。

## #90 #48 B1 emoji→emoji:// 链接改写器的三处口径:覆盖集取 emoji_data 全表 272 枚(参数注入 md 层)、豁免扫描走 pulldown 事件区间(而非手写围栏开关机)、载荷原文直书尖括号目标(而非百分号编码)(2026-10-04,#48 emoji-color B1·自动拍板)

- **岔路**:任务书把三处自由度交给实现者:①**覆盖口径**——只改写 `emoji_data` 覆盖的枚,还是全量 emoji 码位段(任务书明示二选一记本表);②**豁免扫描机制**——任务书说「照 inline_image_dests/wikilinks 先例」(两者都是手写 ```/~~~ 围栏开关机),但同段又要求行内代码与链接文本/目标内豁免,先例机制并不覆盖后两样;③**载荷编码**——emoji 原文 URL 编码或等价可逆编码,口径自定。
- **备选**:①全量 Unicode emoji 码位段(RangeTable 式判定,面板外枚也改写);②app 侧手写扫描器(围栏开关机 + 自造反引号配对 + 括号配对);③百分号编码 `emoji://%F0%9F%98%80`。
- **自动选择**:①**latermd-md 新纯函数 `expand_emoji_links(text, covered)`,覆盖集 = `emoji_data::covered_glyphs()`(面板数据表全量 272 枚,含旗帜/带 FE0F 的两字符形态,前缀长优先匹配),app 侧 OnceLock 注入**;②**豁免区间走与渲染同一套 pulldown-cmark(同 vendored options)的事件区间**——CodeBlock(围栏+缩进+info string)/Code(行内)/Link/Image(构造区间含文本与目标)/Html 块与行内标签/脚注引用与定义,首字符快路径无命中不解析;③**载荷原文直书 `[😀](<emoji://😀>)`**,可逆 = 剥 `emoji://` 前缀。
- **理由**:①覆盖集外的字符改写后无纹理可画——B2 的回落是「透明占位不动」,等于该 emoji 从预览**消失**;只有 272 枚有 #47 的 Twemoji 资产(272/272 在位),覆盖集与面板/纹理共用单一数据源,改表自动跟随;参数化注入让 md 层不反向依赖 app 的数据表,归属与 wikilinks/inline_marks 同层。②手写开关机对「行内代码/链接内」豁免要自造 CommonMark 括号/反引号配对,且与渲染器判定漂移——缩进代码块、跨行链接文本、引用式链接都会判错(实测 pulldown 对这些全部给对区间);pulldown 是铁律 1 的唯一解析器,LP2-1 `inline_marks` 已是同款先例;代价是含 emoji 文档每次快照重建多一次全文解析(无 emoji 文档走快路径零成本)。③尖括号目标允许除换行与 `>` 外的一切字符,覆盖集单测钉住不含这些字符(`covered_glyphs_round_trip_through_emoji_links`);与 `wiki://中文目标` 同口径,渲染串人可读;百分号编码要多一对 encode/decode 面且没有要解的冲突。**已知边界(如实)**:链接引用定义 `[😀]: url` 的标签 pulldown 不产事件、不豁免(极罕见,出现时该行变可见文本);流式未闭合 `[` 后紧跟 emoji 的帧会照改、预览多显示一个 `[`(下一帧闭合即自愈);脚注定义整个区间(含正文)保守豁免,定义正文里的 emoji 暂为黑白。
- **如何改**:①要全量码位段——`latermd_md::expand_emoji_links` 的 `covered` 参数改传按码位段展开的集合(或把参数改成谓词),`state.rs` 注入点同步换;②要手写扫描——替换 `latermd-md` `emoji_rewrites` 实现,并补缩进代码块/跨行链接/引用式链接的豁免测试;③要百分号编码——改 `emoji_rewrites` 内 `replacement` 的 `format!` 一处,并与 B2 的 `emoji://` 解码侧同步。

## #89 #24 C3 快速打开浮层的两处落地口径:选中 QuickOpen 自身只发一条翻转(不补第二条)、浮层高度自适应内容但封顶 430(宽度才定死)(2026-10-03,#24 quick-open C3·自动拍板)

- **岔路**:任务书规定「选中命令 → outbox.push(cmd.message()) 并关面板」与尺寸铁律「窗口宽度定死(如 560px,resizable(false) 或 exact-size 手法)」,落地各有一处自由度:①**命令集里含 `Command::QuickOpen` 自己**——照字面「push 消息 + 补一条关面板」会发两条 `ToggleQuickOpen`(它的 message() 就是这条翻转),同帧先后归约 = 关了又开,浮层关不上;②**「宽度定死」之外高度取什么**——严格定高(浮层恒 430px,短列表空撑一大截)还是自适应内容高度(需自证有界,不违反「禁无界组合」铁律的字面——铁律只钉宽度与列表限高)。
- **备选**:①QuickOpen 自身改为「关面板一条在前 + cmd.message() 一条在后」(顺序避开双翻)、或引入专用 `QuickOpenClose` 消息、或浮层候选里干脆排除 QuickOpen 命令;②严格定高 430(内容尾部 allocate 剩余高度)、或高度完全交给内容(不设上限,靠组上限 8 自证有界)。
- **自动选择**:①**选中 QuickOpen 自身只发 `cmd.message()` 一条,不补关面板那条**(其余命令照旧补一条 `ToggleQuickOpen`);②**`fixed_size([560, 430])`:宽度由此定死(输入框 `desired_width(f32::INFINITY)` 撑满,实测恒 560);高度端 egui 对不可调尺寸窗口本就按内容自适应(`Resize::end` 的「Probably a window」分支),`WINDOW_H=430` 作为内容布局区上限封顶,短列表自然收缩**。
- **理由**:①两条同帧先后归约无顺序能避开「关→开」(翻转幂等路径只有「只发一条」);专用消息要动 C2 已合入的 Message 表与归约(扩表面积);排除 QuickOpen 让「浮层能执行命令全集」的任务字面失真——「快速打开」本身也是可执行命令,且它恰好是「再开/已开就关」的自然语义(与 VS Code 同款 toggle)。回归锁:`command_rows_execute_via_message_and_close` 断言 Enter 后 outbox 恰为一条 `ToggleQuickOpen` 且浮层净效果为关。②严格定高在两三条结果的常见场景里留一大块空白,观感差;而「有界」由三层保证——每组条数上限 8(可见行 ≤ 18)、列表 `ScrollArea::max_height(320)`、布局区封顶 430——高度不随文件总数增长,`window_stays_fixed_and_inside_viewport_with_long_list` 钉死「文件 40 → 80 浮层 rect 逐项不变、宽恒 560、高 ≤ 430、rect 含于视口」。
- **如何改**:①要「关面板」语义更显式——给 Message 加 `QuickOpenCloseRequested` 变体(归约只关不开),浮层对全部命令统一 push 它;②要严格定高——浮窗内容尾部 `ui.allocate_space(ui.available_size())` 把 min_size 顶满,或把测试断言改为恒等高度后调整 `WINDOW_H`;要改宽度/限高数值,只动 `ui/quick_open.rs` 的 `WINDOW_W`/`WINDOW_H`/`LIST_MAX_H`/`MAX_PER_GROUP` 四个常量。

## #88 BK2 反向链接面板(#15)的四层口径:触发=帧末快照比对、点击=WikilinkClicked 同链路(载荷=来源剥后缀)、摘要在扫描层产出、内容变更不重扫(2026-10-03,#15 backlinks BK2·自动拍板)

- **岔路**:任务书定了「当前文档变更/仓库根变更时防抖触发扫描」「点击走 Message::WikilinkClicked → open_wikilink 同一条打开文档路径」「每条目显示来源文件名 + 命中行摘要」,落地仍有四处自由度:①**触发怎么埋**——在打开/切换/保存/另存/换根逐入口插桩,还是统一快照比对;②**点击载荷怎么造**——`Backlink` 有来源确切的相对路径,发携带绝对路径的消息直开(如 `SearchResultClicked`)还是按任务字面拼一个 `[[..]]` 风格目标串走 `find_by_name`;③**命中行摘要从哪来**——`latermd_search::Backlink`(BK1)只有路径/行号/目标原文,行文本要么 app 层重读文件、要么扫描层顺手产出;④**重扫的粒度**——文档「内容」变化(敲字)要不要也触发。
- **备选**:①逐入口插桩(`open_path`/`switch_active`/`save_to`/`change_file_tree_root` 各加一行);②面板行点击发 `Message::SearchResultClicked(root.join(path), line_no)`(`open_search_hit` 已实现打开/切标签/读失败弱提示/跳行全套);③app 层渲染时按 path+line_no 重读文件取行(UI 线程 IO);④触发键加 editor revision,内容一变就重扫。
- **自动选择**:①**帧末快照比对**——`State::end_of_logic` 把「(文件树根, 当前文档路径)」与 `BacklinkState::requested_for` 比对,变了才 `invalidate` 顺延 300ms 防抖(与 `layout_written` 比对写盘同手法);根或文档缺失时 `reset`(不再排程,面板给引导提示)。②**遵循任务字面走 WikilinkClicked**——载荷 = 来源相对路径剥一个 `.md`/`.markdown` 后缀(`jump_target`:`a.md`→`a`、`sub/b.md`→`sub/b`),与 `[[..]]` 目标写法对称,`find_by_name` 的「带 `/` 按相对路径直取 / 否则 stem 忽略大小写」两把钥匙都吃得下;来源已打开时 `open_path` 按路径去重切标签,来源被删时 `find_by_name` 落空 → 提示行弱提示,不崩溃。③**扫描层产出**——`Backlink` 增 `line_text` 字段(BK1 纯加法,扫描时全文在手,按行号折算零成本),app 复用侧栏既有 `SNIPPET_MAX_CHARS`(120 字符)截断,UI 线程零 IO。④**内容变更不重扫**——反向链接匹配的是路径不是正文,「谁链接了我」不因我敲字而变;为每次键入全仓重扫不值得,真需要时换文档/换根的自然重扫已覆盖绝大多数场合。
- **理由**:①快照比对小表面积且**不可能漏入口**——后来新增的任何文档变更路径(将来的拖拽打开、MCP 写回)自动被覆盖,逐入口插桩漏一处就是「面板显示旧文档的反向链接」的静默错误;比对在帧末跑,与防抖顺延天然合并。②复用 `open_wikilink` 即复用「无根提示」「找不着提示」「孤儿 draft 检测」(`spawn_tab` 内)全部既有语义,面板零新路由;`jump_target` 的剥后缀口径与 BK1 `strip_md_extension`(#87)一致,反向收进来的写法正向都打得开。③重读文件在后台线程也行,但扫描层已经拿着整份 lossy 串,行文本是现成的;面板与搜索面板(`SearchResult.line_text`)同构。④见岔路④。**已知并接受的边界**:点击**不跳到来源的命中行**(open_wikilink 只开文档,面板行的行号仅作展示);无 `/` 载荷按 stem 全库找,同名文档多篇时可能切到另一篇(`find_by_name` 列表首个命中,#26 既有模糊性);`find_by_name` 受 `MAX_LIST_ENTRIES`(500) 截断,库尾的同名来源可能找不到;**面板列表不随库内容实时刷新**——别的标签里给第三篇文档加链接,当前文档的反向链接要等下次目标变更才更新;扫描为一次性直扫,大仓首扫期间的防抖窗口内多次变更会合并为最后一次(见下条测试)。
- **如何改**:①要逐入口精确触发——把 `sync_backlink_target`(crates/latermd-app/src/state.rs)的比对键换成各入口显式调用;②要点击直开确切文件并跳行——面板行改发 `Message::SearchResultClicked(root.join(link.path), link.line_no)`(`open_search_hit` 已有全套语义,还能跳到命中行),代价是脱离任务指定的 WikilinkClicked 链路;③要摘要在 app 层现算——revert `Backlink.line_text`,渲染时后台读文件(代价:UI 或归约线程 IO);④要内容级实时——触发键加 `(tab_id, editor.revision)`,每帧比对(代价:每次键入全仓重扫,防抖 300ms 只是缓冲,不建议);要面板手动刷新——加一个「重新扫描」按钮发 `Message::BacklinksRequested` 即可(载荷本就取当下状态)。

## #68 标签「缩短标题/完整标题」(#37)的三个口径:作用范围=整条标签条、默认=完整(既有观感)、收窄分配=保底+max-min 公平(2026-10-01,#37 tab-management 标题宽度显示模式·自动拍板;原编号 #67,因重命名条目重编号顺延为 #68,撞号事由见 #67 条头注)

- **岔路**:任务书规定作用范围「以 docs/decisions-pending.md 本轮规划条目为准(单标签或整条标签条),找不到条目则按『整条标签条』最小方案执行并补记决策」——本轮开工前全表 grep「标题宽度/显示模式/缩短/宽度模式/完整模式」无相关条目(最新条目为重命名的别名语义条,时记 #66,后递改 #67),兜底条款触发,作用范围按整条标签条执行;但落地仍有三处自由度:①**默认模式**(缩短还是完整);②**收窄的分配算法**(Chrome 式全部等宽 vs 按各标签完整宽封顶的 max-min 公平分配);③**最小宽数值**(任务书只说「留合理最小宽」)。
- **备选**:①默认缩短(浏览器原生观感);②等宽平分(所有 chip 一刀切同宽,Chrome tab-strip 行为);③最小宽取仅容「…+关闭钮」的 ~36px。
- **自动选择**:①**默认完整(Full)**——`TitleWidthMode::Full` 为 `#[default]`,旧 settings.json 缺 `tab_title_width` 字段经 `#[serde(default)]` 也回落 Full;②**保底 + max-min 公平**——预算内每个 chip 先保最小宽 56px(不超过自身完整宽),余量按「完整宽低者先封顶退出、剩余标签平分」追加(`ui::tabs::share_widths`);③**CHIP_MIN_W = 56px** = chrome 28px(左右内边距 6+4、关闭钮 12、右内边距 6)+ 约 28px 文本(一到两个汉字 + 省略号)。
- **理由**:①默认完整是不改变 #11 已交付观感的保守取值——缩短模式是用户显式开启的新行为,升级不该顺手改掉所有人的标签条;持久化字段进 `ThemeSettings`(settings.json 既有载荷,主题/emoji_recent 同路),不另开文件;②纯等宽会让「a.md」与长标题一样窄,短标签白白损失辨识度,max-min 让短标签先拿满自然宽、挤压由长标题承担,更贴「按可用空间收窄」的任务书原文;③56 装得下省略号与关闭钮并留一两个字符的辨识余量,预算再紧也不跌破(跌破即关闭钮点不中),溢出交给既有单行水平滚动兜底。切换消息 `TabTitleWidthChanged` 只动 `theme.tab_title_width` + `persist_theme()`,归约测试 `tab_title_width_switch_persists_without_touching_tabs` 钉死「路径/缓冲/dirty/别名/标签 id 逐项不变 + 目录里只有 settings.json」。
- **如何改**:①要默认缩短——`theme.rs` 里 `TitleWidthMode` 的 `#[default]` 从 `Full` 挪到 `Short` 即可(连同旧 settings 回落语义:那是「缺字段=出厂默认」,默认改了回落跟着改);②要等宽平分——`share_widths` 删掉按 cap 封顶的排序循环,直接每项取 `min(full, budget/n)`;③要调最小宽——改 `ui/tabs.rs` 的 `CHIP_MIN_W` 常量(注意保持 > CHIP_CHROME + 省略号宽,否则窄窗下关闭钮被挤);④想给单标签独立设宽——需把 `TitleWidthMode` 从 `ThemeSettings` 挪进 `TabState`(每标签一份)并在右键菜单按被右键标签读写,与「重命名」别名同层,但会失去「一条标签条一种节奏」的整体观感,不建议。

## #67 标签「重命名」(#37)的语义:显示别名而非真实文件改名——非法名只拒空白、别名可重复不查冲突、不持久化、作用范围=标签条 chip 与关闭确认模态(2026-10-01,#37 tab-management 标签重命名·自动拍板;**编号说明**:本棒自记 #66,与本文件已合入的「查找条 Esc 关闭」条目(#17 M1,随 PR #83 进 main)撞号——彼时该条追在文件末尾且文件头「最新条目」行未同步(仍写 #65),自记时未见撞号;2026-10-01 文档收尾递改本条为 #67、标题宽度条顺延 #68,Esc 条目保持 #66 不动)

- **岔路**:auto-plan #37 行只写「实施时明确『重命名』是实际文件改名还是标签显示别名」,本轮规划未在 decisions-pending 拍板(全表 grep「重命名/改名/别名/rename/alias」无相关条目,最新条目 #65 是孤儿 draft 恢复条)。任务书据此规定「找不到该条目则按『显示别名』最小方案执行,并把岔路/选择/理由补记新编号」——语义已由任务书定死为别名,仍留下四处落地自由度:①别名的「非法名」边界(任务书为文件改名列的「空/含路径分隔符/前后空白」是否照搬);②文件改名的「同名冲突拒绝」在别名语义下的对应物;③别名的作用范围(哪些 UI 位跟随别名);④别名是否持久化。
- **备选**:①照搬文件改名的全部非法名(含拒绝 `/` 与路径分隔符);②别名也做同名查重拒绝;③作用范围扩大到窗口标题/状态栏/文件树;④别名随标签持久化(会话重启存活)。以及上位的另一大岔路:真实文件改名(协调路径去重/文件树/保存目标/draft 迁移,任务书 1) 的完整清单)。
- **自动选择**:①**只拒空白**——草稿 trim 后为空即拒绝(浮窗保留、提示行说明,UI「确定」按钮在空草稿时已禁用,归约兜直发消息);`/` 等路径分隔符**不拒**,因为别名是纯显示文本、永不落盘,没有「拼进路径」的攻击面,照搬文件名的非法表只会让用户困惑「为什么浏览器标签能叫 a/b 这里不行」;前后空白 trim 掉(显示文本留首尾空白毫无意义)。②**别名可重复**——两个标签允许同名别名(单测 `tab_rename_duplicate_alias_is_allowed` 反向钉死),「同名已存在即拒绝」防的是文件系统覆盖,纯显示文本没有可覆盖的东西;别名与路径解耦后,同一路径在不同会话/不同标签可挂不同别名,互不牵连。③**作用范围 = 标签条 chip 文本 + 脏标签关闭确认模态文案**(模态问的是「关闭这个标签」,用户在标签条上认的是什么名字模态就问什么,`tab.display_name()`);**窗口标题、状态栏、文件树、保存/另存为对话框一律不跟随**(仍显示文件名)—— 窗口标题与状态栏表达落盘身份,且是别名盖住 chip 后用户找回真实文件名的锚点;浮窗内常驻一行「文件:<名>(重命名不改动它)」明示作用范围,兼作文件名的可视锚。④**不持久化**——标签本身(开过哪些文档)就没有会话持久化,单独持久化别名会造出「重启后别名还在、标签没了」的孤儿数据;别名随标签生灭(会话内状态)。
- **理由**:别名是最小方案,零文件系统操作即零失败面(任务书 1) 的「文件系统失败 → 路径/缓冲/dirty 全部保持不变」由「根本不做文件操作」平凡满足);真实文件改名要协调四件事(路径去重口径、文件树刷新、保存目标、#18 draft 迁移),任何一件漏掉都是数据事故面,而用户诉求(「这个标签我想叫它初稿」)用显示层就能满足,要改真名用户有文件树/系统文件管理器。落点:`TabState::alias`(#37 纯显示层)+ `TabsState::rename` 浮窗状态(稳定 id,`remove` 时与 `confirm_close` 同款撤下)+ 归约 `request_tab_rename`/`confirm_tab_rename`(crates/latermd-app/src/state.rs)+ 浮窗 `ui::tabs::rename_dialog` + chip 显示名改走 `tab.display_name()`。别名置入前后 path/缓冲/dirty 逐一不动由 `tab_rename_sets_alias_display_only`/`tab_rename_rejects_blank_name` 钉死;保存目标不跟随由 `tab_rename_alias_keeps_save_target_and_disk_untouched` 钉死(Ctrl+S 仍写原路径、目录里不出现别名文件);`#[37 别名不改窗口标题]` 由装配测试 `rename_dialog_wired_into_overlay_draw` 钉死。
- **如何改**:①若想要真实文件改名——新开一棒实现 `Message::TabFileRename`(走任务书 1) 全清单:无覆盖目标、同目录名字校验、失败不变式、成功后 `find_by_path` 口径不变但路径换新值、文件树刷新、保存目标切换、`named_draft_path` 草稿迁移),与本别名并存(右键菜单里「重命名」管别名、「在文件管理器中改名」管真名,或把本条目回删);②想禁止 `/` 入别名——`confirm_tab_rename` 加 `name.contains('/')` 分支拒绝即可(连同 UI 侧 `rename_dialog` 的 ready 判定与两条单测);③想让窗口标题跟随别名——titlebar.rs 与 layout.rs 的 `window_title()` 调用换成 `tab.display_name()` 前缀拼接,并同步删装配测试的「不跟随」断言;④想持久化——需先给标签会话(路径列表+active)建持久化文件,别名作为其字段随行,单独做别名持久化无意义。

## #65 孤儿 draft 恢复条(#18)的五处口径:直接编辑即撤条且 draft 保留、读取失败撤条留现场、丢弃无确认、行内条而非浮层、相对时间文案(2026-10-01,#18 孤儿 draft 恢复条·自动拍板)

- **岔路**:①任务书只说「用户无视恢复条直接编辑时 draft 保留」,没说条本身去留;②「恢复」读 draft 失败(坏内容/权限/外部删走)后条的去留;③「丢弃」是否要确认模态(删盘上文件不可逆);④恢复条形态——编辑区顶部行内条(下推编辑器)还是浮层;⑤「保存时间」文案——本仓无 chrono/time 依赖,绝对时刻格式化无依赖可用。
- **备选**:①条一直留到显式点「恢复/丢弃」;②失败后条保留可重试;③丢弃前弹确认;④浮层(Area)或对话框;⑤引 chrono 格式化本地时刻 / 显示 unix 时间戳。
- **自动选择**:①缓冲一旦变脏(帧末 `autosave_pass` 判定)即撤条、**draft 文件不删**(停顿/切出路径照常另写);②失败撤条、draft 文件保留现场、提示行带路径与原因;③无确认直接删,提示行留痕(`已丢弃未保存草稿(<路径>)`);④行内条,画在标签条/提示行之下的 CentralPanel 流里(与提示行同族);⑤相对时刻纯函数(`draft_saved_label`:刚刚保存 / N 分钟前 / N 小时前 / N 天前,mtime 缺失或时钟倒流一律「保存时间未知」)。
- **理由**:①条的存在意义是「盘上版本 vs 遗留稿」的二选一,用户动手编辑就是投了前者,继续挂着只会诱导一次「拿旧稿盖掉新稿」的误点;draft 保留是任务书明文,防丢镜像交停顿路径接管(单测钉了 30s 后新缓冲覆盖旧稿);②坏内容是确定性失败,每次点都一样,留一个永远兑现不了的按钮不如一次说清;文件保留现场,用户尚可手工捞;③丢弃的是「从未保存过的内容」,拒绝它等价于它从未发生,盘上版本分毫未动——无可撤销是因为无可损失,加确认模态反而把最常见路径(打开文档→不想要旧稿)变重;④恢复条是需要持续在场的裁决入口,行内条不可被滚动顶走、不与查找浮层/对话框抢 z 序,位置语义与提示行一致;⑤一个展示字段不值得引入时间依赖,相对时刻对「这稿子多新」的判断也比绝对时刻更直接。
- **如何改**:①想改为条长挂——删 `autosave_pass` 循环里 `if tab.recover.is_some() && tab.editor.is_dirty()` 那三行(state.rs);②想改为失败可重试——把 `recover_draft` 的 Err 分支里的 `tab.recover = None` 删掉即可;③想加确认——仿 `confirm_close` 的稳定 id 模态,确认后才调 `discard_draft`;④想改浮层——把 `recovery_bar` 的调用从 CentralPanel 流里挪进 `draw_find_overlay` 同款的 Area(锚 `source_rect`);⑤想显示绝对时刻——给 latermd-app 加 `chrono`(需登记 ADR-004 依赖清单),`draft_saved_label` 一处替换。



## #64 自动保存「切标签(切出)」是否覆盖「开新标签」(#18 autosave-core·自动拍板)

- **岔路**：任务书原文「当前标签 dirty 且【距上次改动 ≥30s 常量 或 本帧发生切标签(切出)】→ 写 draft」。多标签 #11 的既有语义是「打开文件**永远开新标签**、新建也开新标签」（`TabsState::open_tab` 直接推进 active，不经 `switch_active`）——「开新标签」是否算「切出」直接决定打开另一文件瞬间原标签的脏缓冲有没有 draft 保障。
- **备选**：①只认显式切换入口（标签条点击 `TabActivate` / `Ctrl+Tab` 的 `TabNext`），开新标签不算；②两类都算：`switch_active` 与 `spawn_tab` 都记切出标签 id，帧末统一判定。
- **自动选择**：②。`State::autosave_switch_out` 单槽记录切出方的稳定 id，两个入口都写它。
- **理由**：切出的防丢语义是「标签不可见后缓冲失去看护」，打开另一文件时原标签同样不可见，与点了标签条没有本质差别；只认①会在「打开文件」这个最高频的离开路径上漏掉保障。单槽的局限（同帧连切两次只记最后一位切出者）由停顿路径兜底，不为罕见序列引入 Vec。
- **如何改**：若认为开新标签不该触发（保持①）——删 `spawn_tab`（crates/latermd-app/src/state.rs）里的 `self.autosave_switch_out = Some(...)` 一行即可；若要覆盖同帧连切——把单槽换成 `Vec<u64>` 并在 `autosave_pass` 里去重消费。

## #63 未命名文档 draft 的文件名形态:标签 id 而非时间戳/内容哈希(#18 autosave-core·自动拍板)

- **岔路**：规格只定了「未命名文档落 `config_dir()/drafts/`」，没定文件名。同一会话可开多个未命名标签，文件名形态决定它们是否互不覆盖、以及用户翻状态目录时看副不看得懂。
- **备选**：①`untitled-<标签id>.latermd-draft`（`TabsState::next_id` 发放，稳定不复用）；②时间戳；③内容哈希。
- **自动选择**：①（`State::untitled_draft_path`，crates/latermd-app/src/state.rs）。
- **理由**：标签 id 稳定且不复用（#11 的既有不变量，undo/光标隔离同款前提），同一标签的 draft 永远写同一位置、多标签互不覆盖；时间戳每次落盘换文件名，会积累一串孤儿；内容哈希同一标签连续编辑也换名，还把「防丢镜像」变成了「版本历史」（那是 Git 的职责，不是本功能）。代价：id 数字对用户无语义，崩溃后孤儿文件只能靠恢复条（下一棒）按「存在即候选」处理，不能按文件名猜时间。
- **如何改**：想让用户翻目录时能看懂——文件名加首写时刻（如 `untitled-3-20261001.latermd-draft`，id 前缀保持唯一性），改 `untitled_draft_path` 一处即可；想按时间找孤儿——在写 draft 时顺带维护 `drafts/index.json`（id → 路径与时刻），恢复条消费它。

## #61 代码块复制按钮(#38)的六处口径:挂载点上游已有零 vendor、常驻显示、语言标签取 info 首词、空块两形态、反馈按内容指纹、无语言块也留按钮(2026-09-30,#38 codeblock-copy·自动拍板)

- **岔路**:①甲案预设「在 vendored 代码块 widget 头部加可选 copy 按钮挂载点(独立 vendor: commit + 登记 vendor/README.md)」,动手探测发现 subtree 引入的上游代码本就带 `MarkdownLabel::code_block_buttons(Fn(&mut Ui, &str, &str))` 挂载点(subtree 基点 8e58d97 即有,label.rs 两条代码块渲染路径都调用,回调直接给 `(块源文本, 语言)`);②头部 hover 显示 vs 常驻;③语言标签显示完整 info string 还是首词;④空代码块形态(pulldown 对无空行的空围栏不产 Text 事件,vendored parser 无 token、整块不渲染;带空行的空块 text 为空串、块存在);⑤「已复制」✓ 反馈态的键控(widget id 还是内容);⑥无语言块显示按钮与否。
- **备选**:①按任务书字面在 vendor 再造一个 hook(与上游既有挂载点重复、多一个 vendor: commit 与登记行);② hover 才显示;③显示完整 info;④给空块强行造按钮;⑤按 egui widget id 键控;⑥无语言藏按钮。
- **自动选择**:①零 vendor 改动,直接消费上游挂载点(甲案意图——通用挂载点、按钮视觉 app 侧注入、点击经回调出 app 侧——完全被既有 API 满足);②常驻;③首词;④带空行空块留按钮、点击复制空串,无空行空块随上游语义不渲染(只验不 panic);⑤全局单份「最近复制块指纹+到期时刻」存 egui data,按内容认领;⑥无语言块按钮照旧(只是没有标签行)。
- **理由**:①重复造 hook 违反「以现状为准」;上游回调直接给源文本,任务书「经 code_block_spans 切 rendered」的绕路不再需要(少一处偏移换算出错面);②常驻可发现性好(GitHub/Gitea 同款),hover-only 在无指针环境不可达;③完整 info 可带 `title=` 等元数据,不是语言名;④⑤vendored 挂载点的子 Ui id 按帧内序号自动分配,文档编辑后块序平移会让 id 键控的反馈错位到别的块;内容指纹最多让同文本多块同显 ✓(与 ```ai 卡片按指令文本认领状态同款简化,decisions-pending #13 先例),且跨视口(右栏预览与 Live 中央富渲染)同显合理——同一块内容;⑥交互一致性优先于按内容藏控件。
- **如何改**:①若坚持 vendor 侧出按钮皮肤——把 `code_copy_buttons`(crates/latermd-app/src/ui/preview.rs)的绘制逻辑下沉进 vendored ①类改动并按 AGENTS §6 登记;②改 hover-only——在 `code_copy_buttons` 里按 `ui.ctx().input(|i| i.pointer.latest_pos())` 与 rect 的包含关系门控绘制;③改完整 info——`lang.split_whitespace().next()` 换 `lang`;④想让无空行空块也渲染——需 vendored parser 对空 CodeBlock 补 token(①类,另登记);⑤改 per-块反馈——把指纹换成 (tab, 块序号) 二元组键;⑥藏无语言块按钮——`lang.split_whitespace().next().is_none()` 时提前 return。

## #60 切换卡顿(#39 M2)四候选修法的取舍:per-tab 缓存槽位+heal 条件化落地,首屏分段与后台预热不做(2026-09-30,#39 tab-switch-perf M2·自动拍板)

- **岔路**:M1 根因锁定「切换帧 vendored 预览缓存全量 miss」后,任务列了四个候选修法:rendered/heal 按 (tab, rev) 缓存、heal 仅流式启用、大文档首屏分段(视口外延迟)、TextEdit 切换预热;另有「切 tab 后台预热下一文档解析」。实施时发现四者并非并列:①归约/快照侧(rendered/outline)本来就按 rev 缓存(PreviewState 只在 revision 前进时 rebuild),无重复成本可省;②真正的命中障碍是 vendored 缓存全部挂同一个常量 widget id("preview-md"),切 tab 时槽位被另一文档的 hash 覆盖——这可以在 app 侧解(id 加 tab 维度)也可以在 vendored 侧解(缓存搬出 temp memory 做跨文档 LRU);③「大文档首屏分段」必须改 vendored flush_text_range 的 miss 路径(miss 帧没有段高缓存,跳过布局会塌、用估计高度会跳),是①类中等改动且破坏布局连续性;④「TextEdit 预热」经核实不需要——editor.rs 的 TextEdit 早已用 `tab_editor_id`(per-tab),往返天然命中。最大的实测岔路:M1 基线「切换后首帧 9.45ms」与本次改动后 92ms 的对比表面上是回归,实为 harness 同构样本的假象(sample_doc(1)/sample_doc(2) 模板相同,大量段落 token 内容一致,旧路径同 id 换文本时段级 ctx_hash 命中 A 残留缓存,6.95ms;换成同规模非同构文档对照,旧路径首切实测 92.4ms,与 per-tab id 的 91.5ms 持平)。
- **备选**:A 只做 app 侧:per-tab widget id + heal 条件化,首切成本如实报数字,首屏分段/预热登记后续路线;B 连 vendored 一起改:缓存迁出 temp memory 做跨文档 LRU + miss 帧视口外延迟布局(①类,~几百行,含布局正确性风险);C 加后台预热:打开文档时空闲帧预渲染(无免费午餐——成本只是移到别的帧,galley 非 Send 做不到线程池)。
- **自动选择**:A。附带一个顺手的正确性修复:预览 ScrollArea 的 id_salt 同步 per-tab(原常量 "preview-scroll" 下所有标签共享滚动偏移,切 tab 滚动位置互相踩;per-tab 后各标签记住各自的滚动位置——egui temp memory 无按帧回收,已实测跨往返存活)。
- **理由**:①egui 0.36 `IdTypeMap` 实证无按帧 GC(temp 仅在 clear/remove 时清,egui src/util/id_type_map.rs 与 memory/mod.rs begin_pass/end_pass 均不触碰 data),per-tab id 即可让 vendored 缓存跨切换存活,零 vendor 达成任务预期;②首切 miss 是「该文档第一次被渲染」的固有成本,预热只是把卡移位不消除,分帧预热又有布局塌陷/滚动条跳动问题,两者都违反「不做不达标优化」的务实原则;③2000 行口径往返切回 9.38→2.82ms、20000 行往返 41.2ms,稳态帧 594.7→458.5µs(heal 条件化),改善达标(以上为 M2 当轮数字;**2026-09-30 评审补测修正**:原括注「旧路径真实场景同场景为 900ms+ 量级」系机制推断且量级有误——harness [E] 新增旧路径对照行(常量 id 单槽,doc7 覆盖后切回 big 文档)release 实测 20000 行往返切回 72.1ms(单件),900ms 量级实为「该文档从未渲染过」的冷首切(847–927ms),新旧路径同付,本改法改善的是往返与稳态而非冷首切;同轮 release 复跑 2000 行往返 6.29ms(旧机制单件)→2.33ms(整帧)、20000 行 72.1→31.1ms,结论 A 不变);④20000 行首切 926.8ms>100ms 属「不达标」项,按任务要求如实进 notes 并给出后续路线(vendored ①类:miss 帧视口外段延迟布局,或打开时空闲帧逐段预热),不隐瞒不糊弄。
- **如何改**:若要首帧也达标——授权 vendored ①类改动(label.rs `flush_text_range` miss 路径加视口外延迟 + `CachedMarkdownLayout`/`CachedFlushRange`/`StreamingCodeCache` 迁到跨文档 LRU,按 AGENTS §6 三分类独立 vendor: commit 并登记 vendor/README.md);若接受首切一次卡顿的现状——本条可回删;若要恢复滚动位置跨标签共享的旧行为——把 preview.rs `tab_preview_id` 相关的 ScrollArea `id_salt` 改回常量即可(MarkdownLabel 的 per-tab id 不受影响)。

## #59 评审修复棒被禁 commit,却被要求把 README.md 补段送进 commit——以「点名暂存+借道编排收口提交」闭合(2026-09-30,#22 cask-bump 独立评审修复·自动拍板)

- **岔路**:独立评审 finding(medium)指出 M2 docs commit 6932052 声称「失效下载手动路径补 README」,但 README.md「brew 拿不到新版本时」bullet 仍在工作区未提交;按现状推送则补段不随分支走,docs/distribution.md §3.4 的「README 安装节有同款面向用户的说明」在 main 上失引用,finding 开出的修复是「推送前 `git add README.md && git commit`」。但本修复棒被明令「不要自己执行 git commit(提交由编排按模块声明路径收口)」,且编排收口路径集=模块 paths 并集(`.github/workflows/macos-dmg.yml`+`docs/`),不含 README.md——字面守约束则该 finding 修不了;更糟的是编排收口后有全局 `git status --porcelain` 残留检查(本 run 脚本 `.zcode/workflow-runs/dwfrun-a8bf3e81-*.mjs` 的 st2 步),README.md 留在工作树会让整个 run 在收口时 stopRun 中止,push/PR 全部报废。
- **备选**:A 不动 README,summary 如实列剩——run 大概率在残留检查中止,交付报废;B 只 `git add README.md` 点名暂存(内容一字不动),登记本条;修复棒自身在 docs/ 有本条登记改动,修复棒收口提交必然触发,而编排 `commitPathsIfDirty` 的 `git commit` 无 pathspec、提交整个 index,暂存的 README.md 随之入库;C escalate 等人工裁决。
- **自动选择**:B。
- **理由**:①README.md 改动是本 run 自己 M2 模块按任务书第 2 点(明文「与主仓 README 安装节各补一段」)写就,已在 #58 登记为有意改动,评审确认「内容本身已登记、问题仅是未进 commit」——不是他人 WIP,不触「绝不混入」;②「不要自己执行 git commit」的意图是把提交收口权留给编排,`git add` 不在禁用清单(仅禁 `git add -A`),单文件点名暂存恰是 finding 开出的修复动作,最终落 commit 的仍是编排的收口提交;③编排 `commitPathsIfDirty` 先 `git add -- <声明路径>` 再 `git commit`(无 pathspec)是脚本既有语义,点名暂存是让已登记改动随收口入库的受支持通道;④不修则残留检查必中止整个 run,损失远大于「暂存一个已登记文件」的口径争议。
- **如何改**:若认为本棒无权暂存 README.md——`git reset HEAD README.md` 后按 #58 的回退方案处理(回删 README「brew 拿不到新版本时」bullet,失效下载说明以 distribution.md §3.4 为唯一落点);若认可入库但要求独立 commit——README.md 已随本棒收口提交入库,可事后 rebase 拆分;本条在确认 README.md 已入库后可回删。

## #58 M2 文档棒被点名改 README.md,而「改动面只有 macos-dmg.yml 与 docs/」——同文两口径冲突的解读(2026-09-30,#22 cask-bump M2 文档对齐·自动拍板)

- **岔路**:#22 收官的 M2 任务书 2) 明确要求「在 distribution.md 该小节与主仓 README 安装节各补一段」失效下载说明,同一任务书公共约束又写「改动面只有 .github/workflows/macos-dmg.yml 与 docs/」——README.md 两个口径都不包含,直接矛盾(与 #54 同型:模块 paths 与任务文本点名 README 的冲突)。
- **备选**:A 按「改动面」句跳过 README,失效下载说明只落 distribution.md;B 改 README 补段并登记本条;C escalate 等人工裁决。
- **自动选择**:B。
- **理由**:任务 2 是本模块三大目标之一(模块标题即「失效下载说明」),README 是用户侧说明的第一入口,不改则任务 2 只完成一半;「改动面」一句的可辨认意图是排除其余 workflow(同句逐一列出 release.yml/auto-tag.yml/linux-deb.yml/rust.yml)与代码,README 属文档面不在排除语义内;#54 前例里 README 改动只是次要纠偏且彼时 paths 确为 ["docs"],本次不同——README 补段是主交付物。补充内容全部为既有实测口径的复述(universal2 dmg 资产、xattr 命令、tap cron 兜底),无新断言。
- **如何改**:若评审认为该棒无权动 README,回删 README.md「安装」节新增的「brew 拿不到新版本时」bullet 即可,失效下载说明以 distribution.md §3.4 为唯一落点,其余改动不受影响。

## #57 #22 cask 回填落地:与 tap 侧 auto-bump cron 双写同一 cask 文件的去留(2026-09-30,#22 cask-bump·自动拍板)

- **岔路**:docs/distribution.md §3.4 与 auto-plan.md:94 已核实 tap 仓 auto-bump 含 CASKS 表、每小时自动跟版 cask version/sha256,主仓 #22 与其功能重复、「撤与留留后续评审」;但 auto-plan 队列(权威清单 priority-queue-2026-09-30.json)仍排 #22 且本棒任务书明确要求在 macos-dmg.yml 实现 cask 回填步——照做(与文档「重复」结论冲突)还是停手等评审,是本棒第一岔路。次级岔路:任务书要求 `?ref=main` 读 cask,实测 tap 仓只有 master 一个分支、`ref=main` 404。
- **备选**:A 拒做引用 #53 等评审;B 按任务书落地,步内注释与 docs 如实标注双写关系与竞态分析,分支按现状取默认分支;C escalate。
- **自动选择**:B;分支读写不钉名字(GET 省 ref、PUT 省 branch 字段,均落默认分支,现为 master)。
- **理由**:#53 只登记冲突未裁决,队列把 #22 排在 #19 之后且本 run 按队列派工,「后续评审」的执行主体是用户不是本棒;该步失败语义为纯告警,落地后与 tap cron 并存无实际风险——两边写同一文件的 version/sha256 两行且值同源同值(sha256 都取自同一 dmg 文件:本地 shasum == GitHub asset digest),contents API 带 blob sha 原子覆盖,撞车最坏是 PUT 409 → 告警跳过、cron 兜底,双方都写不坏文件;ref=main 实测 404(gh api 「No commit found for the ref main」,2026-09-30),按「任务描述与仓库现状冲突,以现状为准」改默认分支,还顺带免疫未来分支改名。
- **如何改**:若评审结论是撤主仓侧——删 macos-dmg.yml 尾部 cask-bump 步与 secrets.HOMEBREW_TAP_TOKEN,回删 distribution.md §3.4/§4 与 auto-plan.md:94 的双路口径即可,tap 侧无感;若撤 tap 侧 CASKS 表项——本步成为唯一回填通道,须把 HOMEBREW_TAP_TOKEN 从可选升为必配并在 §4 发版清单加巡检项;若要钉死分支,把 GET 换回 `?ref=<分支>` 并给 PUT payload 加 `"branch"` 字段。

## #56 IME 补报红线的「空闲帧」口径:按自动路径谓词镜像重定义(2026-09-30,#19 ime-follow 独立评审修复·自动拍板)

- **岔路**:原任务红线写「只在编辑器持焦点且光标位置实际变化的帧上报;失焦帧/空闲帧不发任何 IME 命令」;而独立评审 finding 1(high)指出「持焦点+光标未动+有输入事件」的帧(keyup/鼠标 motion/preedit 未变更新帧)里 egui-winit 自动路径必然重写 spot 到 TextEdit 左上角(其 lib.rs:1173 第二个触发项即「事件非空」),不补报就留下整帧错位(坤哥症状帧类)。字面守「光标变化才报」= finding 1 不可修,两条指令冲突。
- **备选**:A 字面守红线、不修 finding 1(残留症状);B 把「空闲帧」解读为「无输入事件且无任何位移的帧」,触发判定镜像自动路径谓词(事件非空 ∨ 内容矩形变化即补报),红线意图(失焦不抢位、空闲/动画帧不轰炸)保住;C escalate 等人裁决。
- **自动选择**:B。
- **理由**:本棒指令「逐条修复」评审 finding 即最新意图;自动路径在事件帧**必然**写 spot,补报与之一一对应,不产生任何多余的窗口系统写入(真空闲帧两边都不写,单测 `ime_trigger_requires_focus_and_change_or_auto_path_risk` 钉住);红线同句「光标位置实际变化」改按 caret 条**屏幕矩形**判定后由滚动/重排位移帧字面满足;轮次 3 冒烟实测 37/37 自动写被同帧盖回、0 单飞,选择 B 未引入多余命令(见 [ime-follow-acceptance.md](ime-follow-acceptance.md) §2.2.1)。
- **如何改**:若要退回更严口径,把 `crates/latermd-app/src/ui/editor.rs` 的 `ime_report_needed` 中 `auto_path_will_fire` 项删掉即可(会重新放行 finding 1 的错位帧类,不建议);若想把镜像基准从点空间改为像素空间以封死纯 ppp 变化的理论窄缝,需另读 egui-winit 的 `ime_rect_px` 语义,改前先补对照冒烟。

## #55 IME 位置显式上报只接源码模式,Live Preview 活动块未覆盖(2026-09-30,#19 ime-follow·自动拍板)

- **岔路**:#19 要求「编辑器持焦点的帧,光标位置变化时向窗口系统上报 caret 区域」;但编辑器有两个渲染面:源码模式(`ui/editor.rs` 的 multiline `TextEdit`,本模块 paths 声明的唯一落点)与 Live Preview(`live.rs` 的活动块 `TextEdit`,id `editor_id.with(("live-block", index))` —— 另一文件、另一 widget)。只接源码模式的话,在 Live 模式活动块里组合中文,候选框仍落在块编辑器左上角(egui-winit 自动路径上报的 widget rect),不随 caret。
- **备选**:A 只接源码模式,缺口登记进本文件与结果 notes;B 本棒扩 paths 改 `live.rs`,同手法复用(块内 primary + 块首字符偏移换算全文 caret,记忆键按块挂)。
- **自动选择**:A。
- **理由**:公共约束明文「只改本模块 paths 声明的路径」,`live.rs` 不在声明内;#19 任务文本与 auto-plan #19 行的实现指引均点名 `ui/editor.rs` + follow_char 同源手法(follow_char 本就是源码模式的产物);Live 块的 caret 换算(块内字符偏移 + 块首)与按块记忆键是新增设计点,该独立一棒受全量门禁,不宜顺手混入。
- **如何改**:后续模块扩 paths 至 `crates/latermd-app/src/live.rs`:活动块 `TextEdit` 渲染后复用 `ui::editor` 的 `ime_report_needed` 与 `ImeCaretTracking`(记忆键 `editor_id.with(("live-block", index)).with("ime-caret")`),caret 取 `块首字符偏移 + 块内 primary`,rect 仍 `galley_pos + pos_from_cursor`;落地后回删本条。

## #54 V2 纠偏棒无法触碰 README.md 与 packaging/latermd.rb：模块 paths=["docs"] 与任务文本点名 README 的冲突（2026-09-30，#44 V2 文档状态纠偏·自动拍板）

- **岔路**：#44 V2 任务文本第 4 点点名「README.md 安装节……失实处改真」，V1 失实清单另含 packaging/latermd.rb:13 的 `version "0.1.0"`（R2）与滞后项 L2（README:54/56 安装表无 deb 渠道）；但编排模块声明的可改动路径是 `paths: ["docs"]`，且编排脚本的收口提交只 `git add -- docs`，其后有全工作区残留检查（`git status --porcelain` 非空即安全中止）——任何非 docs 改动留在工作树会让整个 run（含 V3 验收、全量门禁、推送与 PR）报废。
- **备选**：A 越界照改两文件（残留检查中止全 run）；B 只改 docs/，把 R1/R2/L2 登记留后续单独 PR；C 停下上报等主会话。
- **自动选择**：B。
- **理由**：公共约束明文「严禁 git add -A；只改本模块 paths 声明的路径」与「任务描述与仓库现状冲突，以现状为准」；文档纠偏与 README/模板修订本可分离，延后一棒只损失时效，而 A 会把 V3 真机项登记与六项门禁一并报废，C 违反「登记后继续，不阻塞」的既定玩法。
- **如何改**：后续单独一棒（或主会话授权）按 V1 核对表落三处——R1（README.md:8「当前版本 v0.0.1」→ v0.0.3）、R2（packaging/latermd.rb:13 `version "0.1.0"` → 0.0.3，或注明「实际 version/sha256 由 tap auto-bump 维护，此文件仅初版模板」）、L2（README:54/56 安装表补 deb 一行，`latermd_{version}_amd64.deb`，glibc ≥ 2.35 口径见 acceptance-checklist §4.2）；commitSubject 建议 `docs(#44): README/packaging 版本行与 deb 渠道补正`。落完后回删本条。

## #53 #38 核对计划的五处口径：取证双口径与降级边界、失实判定分档、自动/人工验收边界、§3 归档红线、手打 tag 口径（2026-09-30，#38 核对与文档状态纠偏·规划自动拍板）

- **岔路**：#38 要对六份文档（acceptance-checklist / roadmap / distribution / auto-plan / README / p0-acceptance）做「文档说的 vs 仓库实际的」纠偏核对，但取证依赖 GitHub 网络（Release 资产、CI 结论、tap 仓 cask 状态），网络不可达时如何收场、以及「失实」的判定边界没有先例可循。
- **拍板一（取证双口径）**：在线优先——本机 `gh` 已登录（crazykun，repo 权限），`gh release view <tag> --json assets` / `gh run list --workflow=rust.yml --branch=main` / `gh api repos/crazykun/homebrew-ailater/contents/Casks/latermd.rb` 三路实测可用（2026-09-30 本会话验证）。网络不可达时降级为**本仓可证事实**：`git tag -l` + `git ls-remote --tags origin`（tag 存在性）、`Cargo.toml` 的 `workspace.package.version` 与 CHANGELOG.md 小节一致性、tag 指向 commit 的 `git log`（发版时间线）。**降级时明确标注不可证项**——Release 资产存在性、CI 绿、tap cask 版本三者本仓无镜像，宁可留「未验证」也不许用推测补结论。
- **拍板二（失实判定分档）**：只把「与已取证事实直接冲突」的行列为失实（如 README「当前版本 v0.0.1」vs Latest=v0.0.2；roadmap「P0 剩余=首个 Release 发布」vs 两版已发）；「内容不错但滞后/应补」（如 README 安装节未提 deb 渠道）单列一档，不混入失实——纠偏 PR 的修改范围以失实档为刚性，滞后档仅建议。
- **拍板三（自动/人工边界）**：凡验收判据含「人眼观感 / 真窗口 / 三平台实体机 / IME 候选框」的必人工（acceptance-checklist §2-§5 与 auto-plan 人工待办全部属此类）；`cargo test`（latermd-export 逻辑测试、file.rs 字节往返）、`gh` 在线取证、Linux 本机像素采样与 DISPLAY=:0 冒烟可自动，但 **Linux 自动结论不得冒充三平台结论**，文档回填时须注明证据平台。**计数口径（不带因果）**：测试数一律以 `cargo test -p <crate>` 实跑输出为准（latermd-export 实测 `running 9 tests` / `9 passed`，与 p0-acceptance.md:18 自述一致）；grep 只作定位不做计数——它数的是**匹配行数而非用例数**（lib.rs 里每个 `#[test]` 独占一行才使二者同为 9，不可外推到其他文件），且必须转义方括号（本会话同一文件干净重跑：`grep -c "#[test]"`=0、`grep -c "#\[test\]"`=9、`grep -cF "#[test]"`=9）。历史上出现过的 17 无法用这三个变体复现，故不归因、不写因果，只作废该数值。
- **拍板四（distribution.md §3 纠偏范围定死，2026-09-30 计划评审后补）**：§3「首个 Release 步骤」整节按 **:127-215** 计（§3.1 :129 / §3.2 :147 / §3.3 :162 / §3.4 :185-209 / §3.5 :211-215，§4 起于 :217），不再用截尾行号。其中 §3.4 :193-195「**新建** `Casks/latermd.rb`…`sha256` 用真值替换 `:no_check` 占位」与 tap 实测冲突（Casks/latermd.rb 已存在，version "0.0.1"、sha256 已填真值 `cf1194c8…`），按拍板二**属失实档**；§3.3 :178-183 的「windows-11-arm 首编待验」句在 **:182**。整节归档为「已执行记录」时 §3.4/§3.5 一并改写，不留半节新半节旧。
- **拍板五（手打 tag 口径统一，2026-09-30 二次评审后补）**：`git cat-file -p v0.0.1` 与 `git cat-file -p v0.0.2` 实测均为 **annotated tag**、`tagger github-actions[bot]`、message `Release vX (auto)`（object 分别指向 `daf55fb` / `fe15e34`），且 `v0.1.0` 版本号**从未存在**——手打 tag 不是发版常规动作。故凡写「手动 `git tag v0.1.0` 作为发版路径」的行与同 PR 改写的清单矛盾：**p0-acceptance.md:59**（§4 三件事第 1 条）与 :16/:37/:39 并案进失实档，改写口径 = 「链路已跑通为真（v0.0.1/v0.0.2 均由 auto-tag 自动打并自动发布）+ 手动命令降为 distribution.md §3.2 :157-159 的**应急通道**（仅在自动链路故障时作为人工等价物），版本号按实际写不写 v0.1.0。同一 PR 内两文档对同一动作的口径必须一致，不允许「清单说别手打、验收单仍要求手打 v0.1.0」。

- **拍板四的两条归档红线（2026-09-30 第二轮评审后补）**：整节被改写为历史记录时最容易把「还没做」顺势写成「已完成」，与 #38 纠偏目的直接冲突，故 §3 的任何改写受下面两条约束：
  1. **只对真实发生的步骤改过去式，未做/未回填的项必须保留为显式待办**。§3.5（:211-215）三条本会话逐行核对全部未做：:213「README 安装节确认三条路径可执行」实际 README:54/56 仍无 deb 渠道（滞后档）；:214「roadmap 当前位置 P0 打包条目状态更新」实际 roadmap:20/52/54 仍写「待首个 tag 在 CI 上跑通验证」（正是本批要纠的失实）；:215「m0-report 真机项 Win11/macOS 冒烟结果」属 IME 实体机项从未回填。**勾选项留空或写「未回填，见 acceptance-checklist §X」**，禁止随整节归档默认打勾。
  2. **§3.4 末段「auto-bump 空档」（:202-209）是仍生效的操作纪律，不得随 §3 一同历史化**。该段所述「cask version/sha256 不更新 = 用户 brew 拿不到新版本」当前恰为真（tap 仓实测 version "0.0.1"、Latest 已 v0.0.2），是本条 risks ① 的唯一依据。归档时**保留现在时态原地不动**，或整体迁移到本文 §4「后续版本发布」；删之或改成过去式都算纠偏事故。
- **编号冲突备案**：本任务名「#38」与 auto-plan.md:44 的 #38（codeblock-copy 代码块复制）、decisions-pending #38（图片地址包 `<…>`）分属三套编号体系撞号。规划侧不动 auto-plan.md（超本角色权限），核对计划按任务语义执行；主流程若要收编，需先重编号其一。
- **如何改**：若坤哥认为失实档/滞后档边界划错（如希望 README 未提 deb 也算失实），直接改本条拍板二后重跑核对；若 tap cask 停 v0.0.1 想立即修，走 auto-plan #22（cask-bump）或人工待办「cask version+sha256 手动回填」，不在 #38 纠偏 PR 里顺手改（文档纠偏与跨仓操作不混一个 PR）。
- **第三轮评审补全（2026-09-30，v0.0.3 已发布后；#44 同题核对计划，任务名撞号同「编号冲突备案」）**：时序在本条各拍板之后，下列事实刷新前文引用的现状，全部本会话实跑取证：
  1. **最新 Release = v0.0.3，纠偏口径一律按 v0.0.3**：`gh release view v0.0.3` 实测非 draft 非 prerelease、published 2026-09-30T06:01:47Z，10 件资产含 `latermd-v0.0.3-universal2-apple-darwin.dmg` 与 `latermd_0.0.3_amd64.deb`，macOS 每架构 tar.xz 与 source.tar.gz 已清理；`git cat-file -p v0.0.3` 为 annotated（tagger github-actions[bot]，object=fd40701 = PR #74 merge commit = origin/main 头）；`git ls-remote --tags origin` 三版 tag 均在远端。main 分支 `gh run list --workflow=rust.yml --branch=main` 最近 3 次 run 全 success（最新 headSha=fd40701）。任务验收①写的「v0.0.1/v0.0.2 资产存在性」扩为**三版**（v0.0.1/v0.0.2/v0.0.3）。**v0.0.1 资产名单无 deb**（15 件，且含未清理的每架构 tar.xz 与 source.tar.gz）——deb job 2026-09-29 才接入（distribution.md 头注），v0.0.2 起才有 `latermd_{version}_amd64.deb`；核对 distribution §3.3 勾选清单时按版本分开判，勿把 v0.0.1 无 deb 误记为资产缺失事故，也勿把 v0.0.1 未清理误记为清理步失败。
  2. **失实行清单增量**（拍板二口径，均与已取证事实直接冲突）：README.md:8「当前版本 v0.0.1」；packaging/latermd.rb:13 模板 `version "0.1.0"`（v0.1.0 从未存在，tap 真源已 0.0.3）；roadmap.md:20「P0 剩余 = 首个 Release 发布」、:52「待首个 tag 在 CI 上跑通验证」、:54「剩余：上段打包的首跑验证」；p0-acceptance.md:16「Win / mac 待 CI 首跑」（**复合行**：同行的「Windows 产物尚未在任何机器上跑过」是真机项，保留未勾标 blocked_external）、:37「打包分发 🟨 配置就位,待首跑」、:39「剩打包」；distribution.md:182「windows-11-arm 首编能否通过是下一个待验项」（三版资产名单均含 `latermd-aarch64-pc-windows-msvc.zip`，首编已证通过）；auto-plan.md:90「v0.0.1/v0.0.2 均自动发版」（漏 v0.0.3）。滞后档（不混入失实）：acceptance-checklist.md:18（§0.2「tag 形如 v0.1.0」为格式示例，统一为 vX.Y.Z 或实际 tag）、README.md:54/56 安装节无 deb 渠道（v0.0.3 已有 deb 资产）、acceptance-checklist §7.1（:105）填 sha256 的落点与必要性因 tap 自动跟版需重新定性。
  3. **tap cask 已自动跟版，拍板四的风险前提失效**：tap 仓 `Casks/latermd.rb` 实测 version "0.0.3"、sha256 真值 `d86ec30a…`（不再是拍板四记录的 "0.0.1"/`cf1194c8…`）；tap 仓 `auto-bump.yml` 含 `CASKS=("latermd:ailater/LaterMd")` 表、cron 每小时 :23，注释明言「Cask 的 url 是插值模板永不改动，只更新 version+sha256 两行」。故 distribution.md §3.4 :202-209「auto-bump 空档：Casks/ 不在自动范围」**前提已不真**——归档红线 2 的「保留现在时态原地不动」随之作废，改口径为：该段按已扩事实改写（以 tap auto-bump.yml 的 CASKS 表为证，注明空档存在的历史时段与扩表动作），这不是红线 1 担心的「顺手历史化未做项」，而是事实本身变了；红线 1 对 §3.5 三条未回填的约束不变。连带：上方「如何改」中「若 tap cask 停 v0.0.1 想立即修」一句失效；auto-plan.md:93 人工待办「cask version+sha256 手动回填」失效；主仓 #22 cask-bump（Release workflow 里 gh api 更新 cask）与 tap 侧自动跟版功能重复，是否撤条目由后续评审定，#44 核对表如实登记冲突即可，本补全不代撤。
  4. **acceptance-checklist §0.6（main CI 绿）已升级为可自动核项**：§0.1–0.5 维持文件核对 + 本地六项门禁口径，§0.6 从「待你在 GitHub 上看一眼」改为在线取证（命令见第 1 点）。自动/人工边界仍按拍板三：IME/真机装包/wgpu 真机/长跑/拖拽手感一律 blocked_external 不勾；`cargo test` 计数以实跑输出为准（latermd-export 9 passed 系拍板三实测记录，本次核对未复跑 workspace 测试，执行棒须实跑后再引用）。

## #52 E1 内置表的豆腐块清洗：`has_glyph` 在 epaint 0.36.2 失效，改按 cmap 直验（2026-09-30，emoji-plan E1 自动拍板）

- **岔路**：emoji-plan §7 #2 / E3 原计划「`Fonts::has_glyph` 在建表时过滤缺字形条目」。E1 实测：epaint 0.36.2 的 `Font::has_glyph` 对 emoji **系统性误报 false**——`CachedFamily::replacement_face_key` 是「替 `◻`(U+25FB) 提供字形」的那张脸，而 `◻` 恰在 NotoEmoji-Regular 里（Proportional 链上第一个命中），于是 `has_glyph` 判定「该字符的所属脸 == 替换字形脸 → 没字形」，**全表 296 枚 emoji 无一例外报 false**（含明显有字形的 😀 ✅）。这是 font.rs:719-722 那条 TODO（「用户问替换字符本身会误报」）的泛化：凡替换脸拥有的字符都误报。has_glyph 因此**不能**当 tofu 探测器，E3 的原方案作废。
- **改用什么验**：直读 epaint_default_fonts-0.36.2 打包的 `NotoEmoji-Regular.ttf` / `emoji-icon-font.ttf` 的 **cmap format 12 子表**并取并集——skrifa 0.44 的 `Charmap` 每张字体只选**一张**最优子表（UnicodeFull 胜 UnicodeBmp），BMP-only 的旧符号条目若只活在 fmt4 里运行时同样解析不到，故口径必须按 fmt12 而不是「两张表全并」。以该口径核出原表中 **51 枚豆腐块**（Unicode 10-13 的新 emoji，如 🤝 🤔 🥳 🦄 🧀 🦊 🥑），链序修不出缺失字形（「修 fonts.rs 链序」的预案对「字体里根本没有」无效），遂把 51 枚换成**同分类、经同一口径验证覆盖**的旧常见 emoji（😪 😑 😐 😔 😤 ✋ 👪 🛀 🐴 🐹 🐰 🌽 🍅 🍪 🎧 🎤 💐 🌹 🚙 🚚 ⛽ ⛪ 等），并把「动物与食物」原 41 枚超限修正为 40。
- **顺带的规模事实**：清洗后全表 **272 枚 / 8 类**（≥250 下限，低于甲案宣传的 ~300——缺口全部来自出厂字体的真实字形边界，不是偷工）。若要超出厂字体的覆盖，唯一出路是按 #36 乙案引 `emojis` crate **并随包分发全量彩色 Noto Emoji 字体**（一并解决黑白观感），那是另一个量级的决定。
- **如何改**：①E3 的字形目视仍在（真机看渲染是最后一道），但「建表时过滤」应改为「数据表入库前按本条 cmap 口径核验」（脚本一次性，不入库）；②若坤哥想收回那 51 枚（如 🤝），须先给字体链补一张覆盖更全的 emoji 字体（新增打包资产，按 decisions-pending #4 登记），再把对应行加回 `ui/emoji_data.rs`；③可运行 `python3` 按「cmap fmt12 并集」口径对 `emoji_data.rs` 复验（本棒的手工脚本未入库，需要时可从本条记录重建）。
- **E3 落地补记（2026-09-30，emoji-plan E3 自动拍板）**：①本条口径已**运行时化**——`emoji_data::Cmap12`（零依赖手写 sfnt/cmap-fmt12 解析，约百行纯字节运算）+ `GlyphSet::from_font_cmap12` 在面板首帧对全表核验一次并缓存，`has_glyph` 原方案经本棒复测确认作废（同机复跑 0/272 全 false，与 E1 结论一致）；②核验逮住清洗脚本的 **7 枚漏网**——「裸码位 + FE0F」的文本表现条目 🕵️ 🖥️ ⌨️ 🖱️ 🗺️ ⛱️ ⛩️（4 枚增补平面 fmt12/fmt4 全无，3 枚 BMP 仅 fmt4 有而 harfrust 选表只认 fmt12），处置为**运行时剔除（数据保留）**而非改表；③契约测试 `factory_font_cmap12_prunes_exactly_the_text_presentation_entries` 把 7 枚清单写死，egui 升级致覆盖漂移时测试红即按本条口径重核；④键名缺失/坏字节时全量放行兜底（内置字体恒嵌，全空只可能是核验自身失效，报废面板比冒豆腐风险更糟）。若坤哥要求改按「审计并入 emoji-icon-font 的 fmt4」放行 BMP 三枚（⌨️ ⛱️ ⛩️），扩 `Cmap12::parse` 的子表选择即可。

## #51 recents 项对「无末段路径」的显示回退（2026-09-29，#32 I2 文案收敛自动拍板）

- **岔路**：Files 页 recents 下拉项改为只显示末级文件夹名后，`file_name()` 取不到末段的路径（典型：根路径 `/`）怎么显示。A 回退显示全路径；B 照 `root_label` 的占位文案「未选择根目录」；C 这种项根本不进 recents 列表。
- **自动选择**：A。B 语义错误——recents 里的每一项都是用户真实选过的目录，「未选择」是撒谎；C 过度设计——选 `/` 作文件树根是合法操作（`set_root` 不拒绝），悄悄吞掉历史项比显示 `/` 更难解释。且 `/` 自身无中间段，全路径回退不违反「中间段不进 UI」的文案纪律。单测 `recent_label_uses_last_segment` 把该口径钉死。
- **如何改**：若认为根路径项该有专属文案（如「文件系统根」），改 `sidebar.rs::recent_label` 的回退分支并同步上述单测第三条断言。

## #50 b5035d8 误标 test(app) 且直坐本地 main——分支承载与 message 更正（2026-09-29，独立评审 medium 修复自动拍板）

- **岔路**：独立评审发现 b5035d8 标题写「test(app): #31 行号槽收官核验（纯复跑验证）」，实际 diff 是查找条两处功能修复（`layout.rs` 浮层锚点下移避让工具条 + Enter 匹配排除 Shift，13+/4-），且该 commit 直坐本地 main（领先 origin/main 1 个），未走 fix/ 分支 + PR，一次常规 push 即触发 AGENTS §8 禁止的直推受保护分支。修复两者都需重组本地历史（挪分支、改 message），但公共约束明文「不要自己执行 git commit / git push（由编排统一做）」——若修复者不动历史只留文档，编排收尾会把文档追加成新 commit，b5035d8 带错误 message 原样进 PR，问题 1 只修复一半（#47 记录过的同款困境）。
- **自动选择**：沿用 #46/#47 先例由修复者执行本地重组，不 push、不触碰远端：①复用现场已备好却闲置的 `fix/find-enter-position` 分支（原停 9e8e8b5、无上游、零独有 commit，分支名恰覆盖两处修复）承载该 commit；②本地 main 指针 `git branch -f main origin/main` 重置回 9e8e8b5，即刻闭合误推窗口（commit 由 fix 分支引用，零丢失、完全可逆）；③在 fix 分支 `git commit --amend` 仅更正 message 为 `fix(app):` 并如实描述两处修复（树与 b5035d8 逐字节一致，`git diff` 空输出验证；更正后 f1fb25a）。自测三项（fmt --check / clippy -D warnings / test 369 过 0 败）在 f1fb25a 上全绿。本条（#50）留在工作区，交编排以其原流程提交。
- **如何改**：要回「main 领先 1」旧状，`git branch -f main f1fb25a`（b5035d8 仍在 reflog）；要回旧 message，在 fix 分支 `git commit --amend` 贴回原文（内容未动过）；要换分支名，`git branch -m fix/find-enter-position <新名>` 后照常推送开 PR。

## #49 表格底色「不可辨」的修复取值口径与影响面（2026-09-29，独立评审 medium 修复自动拍板）

- **岔路**：独立评审指出 #30 T3 的表头/斑马底色在深色主题事实上隐形（faint `#2A2B2E` vs 正文底 `#292A2D` 每通道差 1，亮度差 ~0.4%；浅色 Δ=(5,4,3) 也弱于导出 CSS th 底 `#f6f8fa` 的 Δ=(9,7,5)），而旧验收判据「逐像素可分」只证明画了、不证明看得见。修复岔路：A 只调 `theme.rs::shell_tokens().faint` 一个 token 的取值（取色链路不动：token → `visuals.faint_bg_color` → vendored `paint_header_fill` + `egui_extras striped` 同源）；B 为表格单独取色（`TableStyle` 加颜色字段或 header/zebra 各配一色），底色对比可独立调但引入新配置面；C 降级为「表格自带边框就够了」直接关底色。
- **自动选择**：A。公共约束明文「不发明新配置面、底色统一取 `faint_bg_color`」；且取值不必发明——**对齐导出 CSS 的既有口径**：浅色直接取导出 th 底同值 `#F6F8FA`（预览与导出同观感，消「预览弱于导出」的不一致），深色按导出暗色分支的对比度口径（th `#161b22` vs 正文 `#0d1117`，Δ=(9,10,11)）得 `#323438`。**连带影响**：`faint` token 的另两个消费方同步变可见——AI 指令卡底（`preview.rs` 的 Frame fill，卡自带 border，底色变清晰是改善非回归）与 egui `Grid.striped`（app 内无使用，已核）。回归防线 `theme::tests::faint_table_fill_is_visible_against_content` 把判据从「像素可分」升级为「每通道 |Δ|≥5 + 浅色与导出同值 + 投影一致」。
- **如何改**：想要更弱/更强的底色，改 `theme.rs::shell_tokens` 两处 `faint` 并同步测试阈值（重跑像素取证见 [table-render-acceptance.md](table-render-acceptance.md) §8）；要给表头与斑马各自独立于 faint 的颜色，属 vendor ①类补丁（`TableStyle` 增可选颜色字段 + 上游论证），单独立项不与本修复混装；要彻底关底色，`default_markdown_style` 与九套预设把 `header_fill`/`zebra_fill` 置 false 即可（能力保留）。

## #48 预览表头/加粗文字呈 accent 蓝的成因与修复面（2026-09-29，#30 T3 像素验收自动拍板）

- **岔路**：#30 T3 像素取证发现预览的表格表头与 `**加粗**` 文字呈 accent 蓝（#6C9FFF），标题 H1–H6 亦然。根因链：vendored 渲染在未注册 `"bold"` 字族时对表头/加粗回落 `Visuals::strong_text_color()`（layout.rs `apply_bold`、table.rs `apply_bold_to_format`），而 egui 0.36 的 `strong_text_color()` = `widgets.active.fg_stroke.color`（egui style.rs），#27 的 `apply_shell_to` 把 active 控件前景设为 accent（theme.rs，按下态按钮用）——三者叠加把正文强调位全部染成链接同款蓝（还与超链接色撞车，强调位看起来像链接）。app 实际已随 U1 装入 Inter SemiBold，只是族名叫 `Inter-SemiBold`，vendored 探测的字面量 `"bold"` 未注册。修复岔路：A 只登记不动（表头底色验收不受文字色影响）；B 注册 `"bold"` 族名别名（挂 SemiBold 同链），表头/加粗按字重渲染、恢复正文色；C 再加 vendor 改动连标题一起改色。
- **自动选择**：B。两行改动落在 app 侧 `fonts.rs`（零 vendor、零新依赖），正是 vendored 设计文档写明的路径（"Use the bold font family when it is registered"）；表头观感与导出 HTML 对齐（`<th>`/`<strong>` 均为正常色加粗字重）；`**加粗**` 恢复 markdown 语义。标题色不动：vendored 标题路径（layout.rs）无条件设 `strong_text_color`，改它属上游行为变更（需走 ①类 vendor commit + 上游论证），且「标题=accent 强调色」本身是可辩护的样式选择，单独立项不与 #30 混装。
- **如何改**：要标题也恢复正常色，在 vendored layout.rs 标题分支去掉 `format.color = strong_text_color()`（①类 vendor 改动，登记 vendor/README.md 变更表）；要回到「强调位全蓝」旧状，删掉 `fonts.rs` 里 `"bold"` 族名注册两行即可。
- **实测回归（2026-09-29）**：B 方案落地时 CJK 回退循环只覆盖 Proportional / Monospace / Medium / SemiBold 四族，漏了 `bold` 别名族——预览标题（heading 经 `apply_bold` 同样切 `bold` 族）与 `**加粗**` 中文整行变方块，正文（Proportional 链尾有 CJK）正常。修复：CJK 回退循环补挂 `bold` 族 + `fonts.rs` 三处单测加断言（链头/emoji 顺序/`has_glyphs` 实测 CJK 命中）。

## #47 097f69a 混装的拆分由修复者越过「commit 由编排统一做」约束执行（2026-09-28，独立评审 medium 修复自动拍板）

- **岔路**：独立评审发现 097f69a（app 滚动回归修复）混入 vendor ①类行高改动（layout.rs / style.rs）与 docs 排版键位计划，且未登记 vendor/README.md 变更表，违反 AGENTS §6.9 拆 commit / 登记要求。修复需重组这个 commit，但公共约束明文「不要自己执行 git commit / git push（由编排统一做）」——而编排脚本的收尾逻辑是 `git add -A` + 单 message 一次性提交（097f69a 正是这么混装的），自身无法执行拆分；若修复者不动历史、只留工作区改动，编排会把改动追加成又一个 commit，097f69a 原样进 PR，「混装」只修复一半，且 PR body 的「非 vendor 改动」声明与事实不符。
- **自动选择**：由修复者执行 `git reset --soft HEAD~1` 后按主题拆三个 commit（`vendor:` ①类行高含补的测试与登记 / `fix(app):` 滚动回归修复，沿用原 message / `docs(plan):` 排版键位规划与队列推进），不 push、不触碰 main——097f69a 未 push 未合入（origin/main 头为 42aa525），纯本地重组、完全可逆。拆分同时补齐 ①类 commit 的合规件：vendored 测试 tests/line_height.rs、CHANGELOG [Unreleased] 条目、vendor/README.md 变更表登记行。decisions 本条（#47）留在工作区，交编排以其原流程提交。
- **如何改**：要恢复单 commit 旧状，`git reset --soft 097f69a && git commit -m <原文>` 即可（内容一字未动，三个新 commit 的树与 097f69a 相同，仅多了补的测试/CHANGELOG/登记）；要把 vendor 改动挪去其他承载分支，对拆出的 `vendor:` commit 照常 `git cherry-pick`——这正是拆分的目的。

## #46 U0/U1/U3 三 commit 的分支承载与 main 指针处置（2026-09-28，独立评审 medium 修复自动拍板）

- **岔路一**：独立评审发现 U0/U1/U3 三个 commit（f3a8435/7460651/499de06）直坐本地 main（领先 origin/main 3 个 commit），违反 AGENTS §8「main 受保护、feature 分支 + PR」流程——后续一次常规 push 即直推受保护分支。修复时这批 commit 装进**一个** feature 分支开一个 PR，还是按主题拆三个分支各开 PR。
- **自动选择**：单分支单 PR（`feature/ui-modernization` 指向 499de06 承载全部三个 commit，本地 main 指针 `git branch -f main origin/main` 重置回 d0c6a13）。理由：三个 commit 本就按主题分开（U0 token / U1 字体 / U3 动效），PR 内逐 commit 可审；U1 依赖 U0 的 token、U3 叠在其后（`ui/format_bar.rs` 被 U0/U1 先后改动），拆 PR 须按依赖串行合并，编排成本翻三倍而无审查收益；AGENTS §6.9「按主题拆 commit」约束的是 vendor 改动，本批零 vendor 文件（三 commit 的 stat：crates/latermd-app、assets/fonts、docs）。
- **岔路二**：本地 main 领先的处置——保留现状（等推送时再处理）vs 立即重置指针。
- **自动选择**：立即重置（在 feature 分支上操作，工作区零变动）。理由：「领先 3」多存在一刻，误 push 直推受保护分支的窗口就多开一刻；指针重置不丢任何 commit（三个 commit 由 feature 分支引用），完全可逆。
- **如何改**：要拆三个 PR，从 d0c6a13 依次 `git branch feature/ui-u0 f3a8435`、`git branch feature/ui-u1 7460651`、`git branch feature/ui-u3 499de06`，按序各开 PR 合并；要恢复「main 领先」旧状，`git branch -f main 499de06` 即可（commit 都还在）。

## #45 U3 动效的两个落点里，浮层淡入由 egui 内建承接 + 切换淡入的时长公式（2026-09-28，ui-modernization §3 U3 自动拍板）

- **岔路一：任务预期「自研约 20 行」覆盖两个落点（编辑/预览切换淡入 + 浮层淡入），但实测 egui 0.36.2 的 `Area` 已内建 fade-in**（egui-0.36.2 `containers/area.rs`：`fade_in` 默认 true，时长即 `style.animation_time`，opacity < 1 时自动 `request_repaint`；`Window` 关闭再打开时经 `visible_last_frame` 重置计时，重复淡入）。浮层侧再自研一份就是和 egui 打架。
- **自动选择**：浮层淡入**零代码**直接吃 egui 内建（时长 0.2s，`style.animation_time` 调 0 即全局无动画，天然满足可访问性要求）；自研部分只落在编辑/预览切换（`ui/fade.rs::crossfade`，封装 `animate_bool_with_time` + `Ui::multiply_opacity`，约 10 行），并用无头测试把两条路径都钉住（要帧 → 收敛 MAX，明暗两套不 panic）。副作用是两处时长差 0.05s（0.15 vs 0.2），肉眼不可辨。
- **岔路二：切换淡入的时长公式**。规格同时写「0.15s alpha 插值」（ui-modernization §2.6）与「时长与 `style.animation_time` 挂钩」，两者默认值不同（egui 默认 0.2s）。
- **自动选择**：`crossfade` 传 `style.animation_time.clamp(0.0, FADE_S)`——默认得 0.15（规格钦定值），调 0 关动画（可访问性总闸），调得更小尊重更快的偏好；调大封顶 0.15（文本编辑器的模式切换超过 0.15s 显得拖沓，且 LaterMD 未暴露该设置的 UI，实际不存在调大的用户路径）。
- **刻意不做**（任务明示 + 本棒确认）：面板开合动效（`show_collapsible` 已自带滑动）、列表项级动效、禅定进出动效（易晕收益低）。
- **如何改**：①要浮层时长也压到 0.15，把 `style.animation_time` 全局设为 0.15（`theme.rs::apply_shell_to` 一行，影响所有 egui 内建动画）；②要切换淡入跟随全局时长（可大于 0.15），改 `ui/fade.rs::crossfade` 去掉 `FADE_S` 封顶；③要加缓动，`animate_bool_with_time` 换 `animate_bool_with_easing`（egui 0.36 提供，浮层侧 Area 用的是 quadratic_out）。

## #44 U1「工具条/标题用 SemiBold」的接线范围与 Medium 的消费者（2026-09-28，ui-modernization §3 U1 自动拍板）

- **岔路一：任务说「工具条/标题用 SemiBold」，但仓库里没有叫「标题」的单一控件**。候选接线性有三处：①自绘标题栏的文档标题（ui/titlebar.rs）；②格式工具条六个形态按钮 B/I/S/H1-H2-H3（ui/format_bar.rs，此前用 `RichText::strong()` 的人造粗）；③Markdown 预览的 H1-H6 标题 —— 而 ③ 在 vendored 层，`egui_markdown_style::HeadingStyle` 只有 `scales`（字号）**没有字体族字段**，接线必须改 vendor（新增字段 + serde + 渲染分支），动的是 §6 的 vendor 改动纪律，超出「只动观感层 token/主题/字体」的本棒边界。
- **自动选择**：本棒只接 ①②（外壳侧，纯 latermd-app 改动）；③ 不做，预览标题字重留待后续需要时按 vendor ①类（上游可合）补丁单独提案。同时把 B/H1-H3 的 `strong()` 换成 SemiBold 真实字重（真实字形的精致度正是 ui-modernization §2.2「精致度最便宜的来源」的本意；人造粗与 SemiBold 叠加会双重加粗，故去 strong 留 family）。`fonts::semibold_family(ctx)` 带回落：`FontFamily::Name` 未注册时 epaint 直接 panic（epaint 0.36.2 `Font::font` 无 fallback），而大量无头 UI 测试不走 main 的 install——探测不到安装标志回落 Proportional。
- **岔路二：Medium 字重注册给谁**。任务要求三字重都注册，但只点名了 SemiBold（工具条/标题）与 Regular（正文）两个消费者。
- **自动选择**：Medium 注册为 `FontFamily::Name("Inter-Medium")`（链结构与 SemiBold 同构：出厂 emoji 链 + CJK 回退挂尾），**暂不接线**——等一个自然的强调档位消费者（如侧栏选中行、页签活动态）再用，避免为找消费者而扩散 UI 改动面。
- **如何改**：①要预览 H1-H6 也用 SemiBold，走 vendor 补丁：`HeadingStyle` 加 `family: Option<FontFamily>`（egui 0.36 的 FontFamily 无 serde derive，需要自定义序列化）+ 渲染侧 `text_format` 取用，按 AGENTS §6 ①类（上游可合）单独拆 commit/PR；②要 Medium 立即有消费者，改 ui/tabs.rs 活动页签或 ui/menubar.rs 顶层项的 FontId.family 后把本条「暂不接线」划掉；③要换字重档位（如标题用 Medium 而非 SemiBold），改 ui/titlebar.rs 与 ui/format_bar.rs 两处 `semibold_family` 调用即可，fonts.rs 注册结构不变。

## #43 U0 出厂色板的分发形态与「亮色侧数值」的来源口径（2026-09-28，ui-modernization §3 U0 自动拍板）

- **岔路一：九套预设色板怎么「随包分发」**。任务原文写「转成 themes/*.ron 皮肤文件…作为出厂预设随包分发」，但 `themes/` 是**用户配置目录**（`~/.config/latermd/themes/`），安装包（cargo-dist 产物）没有「往用户目录铺文件」的机制；把 .ron 当安装期资源打进 `assets/` 也只是把问题挪了个位置——应用启动仍要把它们落进用户目录才能被 `SkinCatalog` 扫到。
- **自动选择**：色值以**字面量编进二进制**（`theme_presets.rs` 的 `builtins()`），启动时 `install_to(config_dir)` 把每套写成 `themes/<name>.ron`——**仅当该文件不存在**（`state.rs::load_preferences` 与 `main.rs` 各一次，测试注入目录由前者兜住）。用户目录里同名文件永远优先：预设是「出厂底稿」不是锁死资产，用户改过的 Nord.ron 不会被出厂值顶掉。
- **理由**：皮肤系统（#8）的一切能力（扫描、选择、导出、分享）都建在「目录里的普通 .ron 文件」上；铺盘让预设零成本复用整条链路，且「不存在才写」保证幂等与用户改动不可侵犯。代价是每套预设进二进制约 1KB（九套合计 <10KB），可忽略。
- **岔路二：MarkdownStyle 的颜色字段是 dark/light 成对，egui-thematic 只给了一套**。它的 9 套预设里 8 套是纯暗色（`dark_mode: true`），仅 Solarized Light 是亮色；而任务点名的九套（Dracula/Nord/Gruvbox/Solarized×2/Tokyo Night/One Dark/One Light/Rosé Pine）与 egui-thematic 的九套（多 Monokai/Catppuccin Mocha、少 One Light/Rosé Pine）**不是同一集合**。
- **自动选择**：暗色侧照抄 egui-thematic 0.1.1 `config.rs` 各 preset 的色值（2026-09-28 从 crates.io 下载源码逐套核对）；亮色侧按各色板的**公开官方浅色变体**配对（如 Dracula 官方 light、Solarized Light 官方值）；One Light 与 Rosé Pine egui-thematic 没有，按各官方 palette 补齐。九套里 7 套暗 2 套亮，每套的两套明暗值都在同一皮肤文件里，外壳主题切换时皮肤自动跟随（`MarkdownStyle` 的成对字段本就为此设计）。
- **附带拍板：色板只覆盖行内代码四色 + 块级圆角/引用条几何**。`MarkdownStyle` 没有「正文底色/标题色/链接色」字段（那是外壳 `theme.rs::shell_tokens` 的领域，批次 C 已定「外壳不随皮肤换色」）；色板对正文的实际影响面 = 行内代码 + 代码块圆角 + 表格圆角 + 引用条形状。任务说「九套预设色板」时若期待的是整套外壳变色，那是批次 C 的口径问题，不是本棒能扩的。
- **如何改**：①要「安装包带文件而非运行时铺盘」，把 `builtins()` 的数据改为 `include_str!` 的 `assets/themes/*.ron`（dist.toml 需追加资源条目），`install_to` 逻辑不变；②要外壳也跟色板变色，等批次 C 重启「外壳随皮肤」讨论，改 `ThemeSettings::apply` 让 shell_tokens 吃皮肤色；③某套色值想换，直接改 `theme_presets.rs::skins` 对应行，**删掉用户目录里的旧文件**再启动即可重新铺盘（或手动改用户目录那份，效果相同）。

## #42 剪贴板图片的取图通道与「文本粘贴优先」的触发口径（2026-09-28，#26 D 段自动拍板）

- **岔路**：image-plan §3.D 写「取图通道优先 egui 内建；若实测 0.36 的内建剪贴板不支持图片，引 arboard 并按 decisions-pending #4 口径登记 ADR-004」。实测结论：**egui 0.36.2 内建剪贴板只出不进**——`Context` 有 `copy_text`/`copy_image`，读侧只有 egui-winit 在 Ctrl+V 时同步 `Clipboard::get()` 查**文本**翻成 `Event::Paste(String)`（egui-winit/src/clipboard.rs 的 `get` 只走 `get_text`；`set_image` 有、`get_image` 无）。因此必须引 arboard；而**怎么触发读取**是第二个岔路：拦 Ctrl+V 按键 vs 只在「V 按下而本帧无 Paste 事件」时兜底。
- **自动选择**：①直接依赖 `arboard` 3.6.1（eframe → egui-winit/clipboard → arboard/image-data 本就在依赖树，`cargo tree -i arboard` 核实；提升为直接依赖零新增编译面，ADR-004 已登记）；②**读不走 egui**，在 `clipboard.rs` 自建「后台线程 + mpsc」三原语（X11 取剪贴板要跟宿主进程握手，是阻塞 IO，不能进归约——与 AI 流式/图床上传同纪律）；③**触发口径**：`ui::layout::reduce` 每帧看 `input.events`——有 `Event::Paste` 说明 egui-winit 已从剪贴板读到文本（文本粘贴，TextEdit 照常插字，图片流程不启动）；V 键按下而**无** Paste 事件 = 剪贴板无文本的可观察形态，此刻才发起图片读取。V 键只读不消费，不给文本粘贴劫持留任何窗口。
- **理由**：拦 V 键（`consume_key`）会抢在 egui-winit 的粘贴判定之前，剪贴板有文本时文本粘贴被劫持成「读图失败」的空弹窗——文本粘贴是编辑器高频路径，图片粘贴是低频路径，低频必须给高频让路。「无 Paste 事件才兜底」利用了 egui-winit 自己生成的信号，两个通道天然互斥、无竞态。另两条已定口径：arboard 返回 RGBA 像素统一**重编码 PNG** 落盘（Linux X11 后端读进来本就是 PNG；扩展名因此恒 png）；剪贴板字节没有原名，合成 `粘贴图片-<纳秒时间戳>.png`（`assets.rs::pasted_image_name`）。
- **如何改**：要「Ctrl+V 一律先问图片」（部分编辑器把剪贴板图片优先级放得更高），把 `reduce` 里的判定改成「V 按下即发起读取，Paste 事件与图片结果同帧到达时丢弃图片结果」——需要给 `ImagePasteFinished` 加发起帧标记，代价是文本粘贴场景多一次无谓的剪贴板读取；要改成 macOS 上读系统剪贴板的文件列表（截图工具落的是文件而非像素），在 `read_clipboard_image` 前先试 `clipboard.get().file_list()`，命中且为白名单扩展名时改走拖拽同款路径。

## #39 图床上传的「成功即自动插入」与上传后关框（2026-09-28，#26 C 段自动拍板）

- **岔路**：图片框「选文件并上传…」拿到图床 URL 后，是**回填 url 草稿等用户点「插入」**，还是**直接自动插入**。
- **自动选择**：自动插入（`state.rs::request_image_upload` 关框并发起上传，`finish_image_upload` 成功即 `insert_image_at`）。
- **理由**：上传动作本身就是用户的明确插入意图（选了文件、选了图床、点了按钮），回填后再要一次点击是多余步骤；而失败路径已有硬约束「绝不动文档」（image-plan §4.3），自动插入的代价只剩「成功那一下没法反悔」——Ctrl+Z 可回退（compose 写入的 undo 粒度是既定已知项 §4.2）。上传中切换标签时结果写回**发起标签**（照抄 AI 流式 `ai_active_tab` 的绑定手法）。
- **如何改**：想改回「回填等确认」，`finish_image_upload` 的 `Insert` 分支改为重开图片框并回填 `image_dialog.url`，插入仍走「插入」按钮。

## #40 图床 profile id 用时间戳而非 uuid v4 形态（2026-09-28，#26 C 段自动拍板）

- **岔路**：image-plan §3.C 规格写 `id: String, // uuid`。是否引 uuid crate 生成 RFC 4122 形态。
- **自动选择**：不引 uuid;`bed.rs::new_profile_id` 用纳秒时间戳 + 进程内计数（`bed-<nanos>-<seq>`）。
- **理由**：id 的全部用途是「beds.json 里的键 + latermd-creds 的 account 键」，需要的是**唯一且稳定**，不是任何特定格式;可读性（能从 id 看出创建顺序）反而是排查凭据条目时的加分项。为此引一个新依赖违反「小表面积」纪律（AGENTS §8）。
- **如何改**：想要 uuid 形态，把 `new_profile_id` 换成 `uuid::Uuid::new_v4()` 并在 latermd-app 加依赖（已在依赖树内，代价很小）;存量 beds.json 的 id 无需迁移（两种形态都是不透明字符串）。

## #41 图床 token 在「无钥匙串环境」的行为（2026-09-28，#26 C 段自动拍板）

- **岔路**：latermd-creds 在无 Secret Service（Linux 裸环境/CI）下读写都失败。图床页「保存」写了 token 但钥匙串不可用时，是**拒绝保存整个 profile**还是**照存 profile、只提示 token 未落**。
- **自动选择**：照存 profile,token 失败只落提示（`state.rs::save_bed_profile`）。上传时凭据读不到 → `${TOKEN}` 替换失败 → `BedError::MissingToken` 明确提示「请在 设置 → 图片 里保存该图床的 token」。
- **理由**：profile 本身无秘密（只有 URL/字段名/占位符），拒绝保存会让无钥匙串用户连「免 token 的自建图床」都配不了（SM.MS 之外的匿名上传端点真实存在）;失败延迟到上传时才暴露，且错误文案已指路。AI key 的处理是同款先例（保存失败降级环境变量，不阻断其它配置）。
- **如何改**：想更严格，`save_bed_profile` 在 token 写失败时回滚 profile 落盘（不进 beds.json），提示「先修好系统凭据再配图床」。

## #37 本地图片「浏览即复制」的孤儿文件取舍（2026-09-28，#26 B 段自动拍板）

- **岔路**：图片框「浏览…」选中的本地图片，复制进 `<doc名>.assets/` 发生在**浏览时**还是**点插入时**。
- **自动选择**：浏览时（`state.rs::pick_image_file`：rfd 选中即复制并回填地址栏）。
- **理由**：url 栏回填的是真实落盘地址（撞名 `-1` 后缀已定），用户所见即所插；插入消息
  `ImageInserted{alt,url}` 保持零改动，三条来源的汇流点不被破坏。代价：浏览后点「取消」会在
  `.assets/` 留一份未引用文件。
- **如何改**：想消灭孤儿，改 `pick_image_file` 只记源路径（`ImageDialogState` 加 `picked` 字段），
  把复制挪进 `Message::ImageInserted` 的归约（复制失败只弹 notice 不插文本）；另需处理「显示的
  url 与实际落盘名不一致」（浏览与插入之间又进了一张同名图）的提示。孤儿文件本身也可用一条
  「清理 .assets 未引用文件」命令兜底（首期不做）。

## #38 机器生成的图片地址遇空格/括号自动包 `<…>`（2026-09-28，#26 B 段自动拍板）

- **岔路**：image-plan A 段定过「URL 含空格或中文时不转义、不自动 `<>` 包裹」；但 B 段复制本地
  文件**保留原名**（规格明文），文件名含空格/括号（`屏幕 截图 (1).png`）时裸目标语法会被
  CommonMark 截断，图片直接不出。
- **自动选择**：A 段口径**只约束用户手填的网络地址**；B 段机器生成的相对地址在含空格/括号时
  自动包 `<…>`（`assets.rs::relative_url`，中文仍裸放），预览侧改写函数同款处理
  （`ui/preview.rs::resolve_relative_images`）。与 `expand_wikilinks` 包 `<>` 是同一先例。
- **理由**：A 段的「不包裹」是为了不替用户改写输入；B 段的地址是我们自己生成的，语法合法是
  生成方的责任。
- **如何改**：若坤哥希望连本地文件也保持裸写，删 `relative_url` 与 `resolve_relative_images`
  两处的包裹分支，并把复制时的改名策略换成「空格/括号替换为 `-`」（改名比留语法残骸干净）。
- **附记（现状冲突，非取舍）**：image-plan §1 写「图片渲染走 vendored 层 label.rs:844」，
  实况是该分支整体在 vendored 的 `images` feature 之后且上游默认关——B 段已在 latermd-app 侧
  启用该 feature 并装 `egui_extras` 图片 loader（file/image，解码格式开到 D 段白名单
  PNG/JPEG/GIF/WebP）；vendor 文件一行未动。

## #35 图标体系是否整体迁移到 egui-phosphor（2026-09-27 坤哥转来外部建议；2026-09-29 拍板乙执行、**同日实机回滚甲**）

- **岔路**：外部推荐 P1「集成 egui-phosphor，替换所有文字按钮为图标+文字组合，工作量低」。
  查证（2026-09-27）：库本身**健康** —— 0.14.0、14 天前 bump 到 egui **0.36**（与我们对齐）、
  ~2900 下载/月、**#65 in GUI**、MIT/Apache-2.0、字体 bundled（1.03 MiB，可 subset 裁剪）。
- **首轮不做的理由（不是否决库，是否决时机）**：① 我们已有 **30+ 枚自绘图标**（`ui/icons.rs`：
  文件组 / 格式组 17 枚 / 标题栏六钮 / 侧边栏四页 / 状态），且 LOGO 也自绘同源（`assets/logo/`），
  已过明暗双主题与像素验收；② **两套图标并存 = 视觉分裂**，是所有选项里最糟的，只能「全换」或「不换」；
  ③ 全量迁移要动工具条 + 文件栏 + 侧边栏 + 标题栏 ≈ **2–3 天**，不是外部估计的「低」。
- **首轮选择**：本轮**不换**。精致度走 `ui-modernization.md` 的 U0/U1（抄 armas 数值进 tokens +
  Inter + CJK fallback），零依赖、约 1 天。
- **拍板与执行（2026-09-29，坤哥指令「合并 feature/ui-modern-all，完成 egui-phosphor 整体图标迁移」）**：
  选**乙**。`feature/ui-modern-all` 上的 ⑥（0327480，2026-09-27）cherry-pick 到 main 头上，落定要点：
  ① 30 个变体人工映射 phosphor 码位（Save→FLOPPY_DISK、Theme→PALETTE 等名字对不上的逐个挑）；
  ② 字形是 bundled TTF 的私有区码位（U+E0xx），三平台一致，「自绘不用字体」的旧论据只对系统字体成立，
  对 bundled 字体库不成立（#34 已修订）；③ 绘制退化成排版一段文本，字体族走 **Proportional** 而非单开
  `FontFamily::Name` —— 后者遇到没跑 `fonts::install` 的 Context 直接 panic，前者只是豆腐块；
  ④ `build_definitions` 里 `add_to_fonts` 落位 **Inter 之后、NotoEmoji 之前**（实测 insert(1)，探针+单测
  `inter_leads_and_emoji_order_survives` 双重锁定），与 CJK/emoji 互不抢；⑤ 补 `Icon::Image`（#26 图片框
  在分支分叉后新增的变体）映射 `ph::IMAGE`；⑥ `egui-phosphor 0.14` 按惯例登记 ADR-004。
  **分流记录**：该分支的 ①⑤（tokens/theme 自研版）、③（Inter 内联正文）、④（按钮 hover 淡入）分别被
  main 已落地的 #27 U0/U1/U3 替代，不随本迁移合入；分支打 archive tag 后删除。
- **如何改（两个选项，存档备查）**：
  - **甲**：不换。需新增图标时继续自绘。
  - **乙（已执行后回滚，见下）**：单开一棒「图标全量迁移 phosphor」，`ui/icons.rs` 自绘退役，与图片/图床主线**串行**做。
  - 回退：revert `da8061a` 即回到自绘版（icons.rs 消费者 API 不变）。
    注意选乙后，`Icon` 枚举的所有消费者（`titlebar`/`sidebar`/`format_bar`/`menubar`）要同步改。
- **实机目视回滚（2026-09-29，坤哥看真机后「这图标还没原来的好看」）**：拍板乙的同一天真机验收 30 枚
  phosphor 图标，观感整体不及原自绘体系 —— 自绘 30+ 枚是按 LaterMD 的线宽/密度/明暗 token 逐枚画的
  （过 #27 明暗像素验收），phosphor 是通用图标集，笔画的粗细节奏与外壳不咬合；映射里还有 Save→软盘这类
  语义让步。**拍板回滚甲**：revert `da8061a`（`ca6e164`），`ui/icons.rs` 回自绘版（含 #26 的
  `Icon::Image`），`fonts.rs`/`Cargo.toml`/ADR-004 依赖同步还原；`egui-phosphor 0.14` 从依赖表移除。
  phosphor 线的完整实现保留在 git 历史（PR #55、`archive/feature-ui-modern-all` tag），将来若要重启
  （如 phosphor 换 weight 变体、subset 裁剪后再评）从那里捡。**教训**：图标这类纯视觉决策，像素验收
  只能证「画了且一致」，「好不好看」必须在真机上人眼拍板 —— 乙的执行成本（迁移+回滚）本身印证了
  首轮「全换要 2–3 天」的估算是准的。
- **2026-09-27 更新**：坤哥指令「根据文档的最近规划，图片，emoji，ui等功能，添加新的流水线workflow」——
  UI 现代化已按**甲**（不迁移）排入流水线（auto-plan #27 已放行，范围 U0/U1/U3，预写脚本待看护自动开棒）；
  U4/phosphor 本条**仍待拍板**，选乙时在 #27 完成后单开一棒。

## #36 Emoji 面板的数据源选型（2026-09-27，坤哥指令「工具栏加入 emoji」，**待拍板**）

- **岔路**：面板里那批 emoji 的数据（字符 + 名称 + 短码 + 分类）从哪来。
- **两个选项**：
  - **甲（推荐 / 当前默认）内置精简表** —— ~300 枚 / 8 分类，中英双语名 + 短码，约 12KB 常量表，**零新增依赖**。
  - **乙 引 `emojis` crate** —— 1MB 数据、1800+ 枚、Unicode v17、含 gemoji 短码。库本身健康
    （0.9.0 / 2026-06 发布 / 37 万下载月 / 144 crates 使用 / 纯数据无 unsafe），许可证
    `(MIT OR Apache-2.0) AND Unicode-3.0` 与 MIT 兼容；但要按 **#4** 登记进 ADR-004，
    且 Unicode-3.0 的署名要求要带进 `docs/distribution.md` 的第三方声明。
- **自动选择（甲）的三个理由**：① 编辑器里实际会点的 emoji 就那几百枚，全量大半用不上；
  ② **隐藏成本** —— epaint 的字形是**渲染过才进字体图集**，全量数据下用户翻一页就把整页
  emoji 渲进图集，反而更容易触发图集扩容/重排（这是选乙容易被忽略的代价）；
  ③ 少一个依赖就少一次 ADR 滚动修订，与「能抄数值的不引库」一致。
- **如何改**：选乙 → `latermd-app/Cargo.toml` 加 `emojis = "0.9"` + 同步 ADR-004 登记一行；
  选甲 → 直接在 `ui/emoji_panel.rs` 写常量表，无外部动作。
- **附一条实测（防以后走弯路）**：网上教程让开 `monochrome_emoji_fonts` 以获得 emoji 字体 —— 该
  feature 在 **0.36.2 已不存在**（已并入 `default_fonts`），照抄编不过。egui 0.36.2 的默认字体链
  **本来就含** `NotoEmoji-Regular`，不需要任何额外动作，也不需要直接依赖 `epaint_default_fonts`。
- **2026-09-27 更新**：坤哥指令把 emoji 排入流水线（auto-plan #28 已放行，预写脚本待看护自动开棒），
  未另行拍板 → 按本条既定默认**甲·内置精简表**执行；事后想改乙，按上方「如何改」操作
  （`emojis = "0.9"` + ADR-004 登记），改后 amend #28 脚本即可。

## #34 ui-polish §1.1「图标自绘」的论据需要细分（2026-09-27，查证后修订）

- **岔路**：ui-polish §1.1 写「图标一律用 Painter 画线段/圆/矩形，零字体依赖」，
  论据是「emoji / Unicode 符号（✎ 🗋 ⌘）在三平台字体下缺字风险真实」。
  用这同一条论据去否 `egui-phosphor`，**是错的**。
- **修订**：缺字风险要按「字体从哪来」分两类 ——
  | 类型 | 缺字风险 | 结论 |
  |---|---|---|
  | 系统 emoji / Unicode 符号 | **有**（三平台字体不同） | 原结论成立，**继续禁用** |
  | 内嵌字体的图标库（phosphor，bundled font bytes） | **无**（字体随 crate 分发，不查系统字体） | 原论据**不适用** |
- **选择**：保留「自绘」作为当前实现，但**禁用/采纳的判断依据改为「返工成本 + 视觉一致性」**，
  不再引用「字体依赖风险」（见 #35）。`ui-polish.md` §1.1 原文的字体缺字论据仍然有效，不改动原文。
- **如何改**：将来若再评估图标库，直接看 #35，不要用「有字体依赖」一句话打发。

## #33 状态栏右端是否放内容（2026-09-27，M5 像素验收发现）

- **岔路**：M5 像素验收断言「状态栏左右端 50px 内均有非纯背景采样」右端不满足——`status_bar`（`ui/layout.rs:526`）是 `horizontal_wrapped` 左起排列，当前内容（文档名 · 行列 · 字数 · 主题 · 模式 · 后端 · AI · MCP）总长约 320px，未触及右端，右端 50px 为纯底色。**底色横跨全窗已像素级验证成立**（900 列连续，仅 4 列因文字笔画边界判非连续），不满足的只是「右端有内容」这半句。
- **选择**：维持左起排列不动。右端放什么（MCP 端口右对齐？git 分支？时钟？）是产品内容取舍，验收棒不加功能（AGENTS §7 范围边界）。验收文档如实记录「横跨 PASS / 右端无内容采样（设计现状）」。
- **如何改**：若要右端有常驻指示，在 `status_bar` 里加 `with_layout(Layout::right_to_left)` 段或用 `ui.allocate_ui_with_layout` 撑开左段后右对齐放元素；改完状态栏跨窗断言的右端子句自动满足。

## #32 编辑器区顶部文件工具栏退役，文件动作收口左栏图标版（2026-09-27，用户指令）

- **岔路**：坤哥 2026-09-27 第二批反馈「文件动作在左栏和编辑器顶各有一排，重复」。左栏顶段（M2 起）与编辑器区顶部文件工具栏（ui-polish 批次的 `ui/toolbar.rs`：新建/打开/保存/另存为/导出文字按钮 + 视图组 + AI 下拉 + 齿轮）并存近一天。
- **选择**：**保留左栏图标版**并核对为最全集（`Command::FILE` + `ExportHtml`，共五枚，tooltip 带出厂键位）；`ui/toolbar.rs` **整体删除**（含 `mod` 声明与 `icons::icon_text_button`/`allocate_button`/`tooltip` 三个仅存工具栏消费者的辅助函数）。AI 三命令不新增入口（菜单栏「AI」菜单本就有，原工具栏下拉为重复）；`ToggleSidebar`/`ToggleTheme` 不重复加（标题栏已有 `┃左`，主题在菜单栏「视图」与设置浮窗外观页三态）。
- **顺带迁移**（工具栏的唯一无主能力）：`document.notice` 提示行（撞键拒绝、保存失败等「知道了」式提示）迁到编辑器面板顶、标签条之下，存在才显示（`ui/layout.rs` 的 `notice_bar`）；文档名与齿轮在标题栏/状态栏已有，无能力丢失。
- **如何改**：想要回编辑器顶部的文字按钮排，从 git 历史恢复 `ui/toolbar.rs` + `ui/mod.rs` 的 `pub mod toolbar;` + `ui/layout.rs` CentralPanel 里的 `toolbar::ui` 调用即可，三处一一对应。

## #31 设置入口从左栏底段挪到标题栏右端齿轮（2026-09-27，用户指令）

- **岔路**：坤哥 2026-09-27 运行实测外壳后指令「设置挪右上」。原 M2 规格（ui-shell-redesign.md §5）把设置放在左栏**底段**（齿轮 + 「设置」一行，左键默认页 / 右键四项直达）。
- **选择**：左栏收回为三段（顶动作 / 视图导航 / 中段视图内容），设置入口改为**标题栏右端齿轮**（最小化按钮左侧，七个窗口按钮中的第 4 个）；左键发 `SettingsOpened(Appearance)`（默认落地页不变），**右键四页直达随迁到齿轮右键**（不丢能力）。`NAV_BOTTOM_H` token 与 `SidebarBands::bottom` 一并清理。
- **顺带修复**（同一批用户反馈，根因同为 panel 顺序）：①四栏黑条——编辑器曾用 `Panel::left`，其后中央残余区无人认领，改回 `CentralPanel`；②状态栏横跨——`bottom("statusbar")` 挪到 nav/preview **之前**画，先画者占满全窗横向。
- **禅定不受影响**：draw_zen 整体分叉，标题栏（含齿轮）按 D4 决策保留，statusbar 照旧退场。
- **如何改**：想把设置挪回左栏，恢复 `ui/sidebar.rs` 的底段（git 历史里有 `settings_row`）并从 `TITLE_BUTTONS` 摘掉 `Settings`；齿轮与窗口按钮同格（`WINDOW_BTN` 命中区），若嫌它混进窗口控制区，可改为独立小图标按钮并加一格间距。

## #30 外壳重构的五个岔路口（2026-09-26，**已拍板落地：M1–M4 合入 main**）

- **岔路**：坤哥指定「左导航 / 中源码 / 右只读」三分栏 + 右上开关 + 禅定。规格全文在 [ui-shell-redesign.md](ui-shell-redesign.md)。其中五处是真正需要人拍的取舍，不像以往那样「自选一个继续」—— 因为 D1 改的是**窗口形态本身**，代价不可逆，不符合本文件「先干再改」的既定玩法，故登记等待而非自选。
- **为什么这次不自选**：#1–#29 的岔路失败后按「如何改」一节回退的成本都在几百行代码内；D1 一旦落成无边框，三平台的 resize/阴影/圆角行为要真机逐项验收（R1），返工要动 `main.rs` 的 `ViewportBuilder` + 自绘层的全部拖拽逻辑。
- **D1 标题栏**：**自绘**（`with_decorations(false)` + 自绘 36px 条，六按钮）vs 保留原生装饰、开关退到菜单栏右端。默认自绘；逃生口 `LATERMD_NATIVE_DECORATIONS=1`。
- **D2 菜单栏**：保留独立行（可发现性优先）vs 并入 ☰（省 28px）。默认保留。
- **D3 左栏形态**：单栏三段式（记号：改动集中在 `ui/sidebar.rs`）vs VS Code activity bar（两个 panel，省横向地方）。默认三段式 —— 只有 4 个视图，不值两个 panel。
- **D4 禅定**：保留标题栏（鼠标移到顶部 2s 淡出，v2 做）vs 全无 chrome（只剩 Esc 退出，可发现性差且与 macOS 全屏手势打架）。默认保留。
- **D5 可视化编辑**：预留接口不实现 vs 现在就动手。默认预留 —— 它是 Live Preview v2，属 P3 剩余项。
- **如何改**：拍板后按 ui-shell-redesign.md §12 的 M1–M5 顺序开工；D1 若选保留原生装饰，M1 退化为「菜单栏右端加三个开关按钮」，省约 1 天且 R1 整体消失。

## #29 SaveAs 撞上已在另一标签打开的路径:写盘前拒绝认领(2026-09-26)

- **岔路**:独立评审指出 `State::save_to` 认领路径前不查重,另存为到已在另一标签打开的路径会破坏「同一路径至多一个标签」不变量(此后两边各保存一次就互相静默覆盖)。修法两派:①写盘前拒绝认领 + 提示行指路(先关另一标签或另选路径);②允许保存并自动接管 —— 写盘成功后关掉另一标签(干净时)或仅在另一标签脏时才拒绝。
- **自动选择**:①**写盘前拒绝**,提示「{路径} 已在另一标签打开;请先关闭该标签或另选保存路径」(`State::save_to` 入口按 `find_by_path` 查重,命中且非本标签即返回,盘上内容与另一标签缓冲均不被触碰)。理由:与仓库既有哲学同构(撞键拒绝、脏标签关闭要确认 —— 静默覆盖与标签凭空消失的代价都大于多一次操作);②的「自动关掉干净标签」虽无丢稿风险,但用户眼里的标签消失同样是意外行为,且要连带处理关谁的 active 指向与在途 AI 流作废,复杂度不成比例。保存到本标签已持有的路径(常规 Ctrl+S / 同路径另存为)不受影响 —— 命中即本标签,放行。
- **已知并接受的边界**:查重走 `find_by_path` 的逐字节路径比较(与打开侧三入口同一口径),不做 canonicalize —— 符号链接、相对/绝对混写、大小写不敏感文件系统上的别名路径查不出(打开侧同样查不出,如要收紧须两边一起)。
- **如何改**:要②的「另存为即接管」,把 `save_to` 的拒绝分支改成写盘成功后对命中标签走 `remove_tab`(另一标签脏时仍拒绝,文案不变);要支持别名路径,给 `find_by_path` 加 canonicalize 并同步覆盖打开侧,防两边口径不一。

## #28 WorkBuddy 风外壳：强调色从紫罗兰改蓝、面板靠底色分区（2026-09-26）

- **岔路**：坤哥看过 WorkBuddy 截图后要求「UI 要这种风格」。冲突点：①既有强调色是紫罗兰（与 `ai://` 链接同源，decisions-pending #11/#24），WorkBuddy 是飞书系蓝；②egui 出厂的 light/dark visuals 是"灰底 + 硬边框"的桌面风，WorkBuddy 是「侧栏灰 #F2F2F2 / 内容白 / **无硬边框**，靠底色分区」；③皮肤系统（#24）只管正文，外壳配色按批次 C 约定"随皮肤换色"是不做的。
- **自动选择**：①强调色改**飞书系蓝**（浅 #3370FF / 暗 #6C9FFF），AI 专属元素（ai:// 链接、指令卡）**保留紫罗兰** —— 强调色中立化后，AI 反而是全界面唯一的紫，更醒目；②新增 `theme::shell_tokens(dark)` + `apply_shell()`：把侧栏 #F2F3F5、内容 #FFFFFF、文字 #1F2329、悬停、浅蓝选区、控件圆角 6 等投影进 egui 的**两套 style**（只投影一次，`style_mut_of` 会推进 style 版本作废布局缓存）；面板分区用底色不用线 —— `noninteractive.bg_stroke` 压到 border 色一档；③外壳是**内置观感**不是皮肤（皮肤仍只管正文），批次 C 的约定不变。
- **实现里踩的 egui 0.36 坑**：①`Visuals` 已无 `window_rounding`/`menu_rounding` 字段，`Spacing` 也没有，浮窗圆角只能走出厂值；②TextEdit 底 = `extreme_bg_color`，而 vendored 代码块底 = `code_bg_color` —— **两者必须分开**（都给灰的话编辑器整片是灰的，"灰侧栏 + 白内容"就没了）：extreme 给内容白、code_bg 给 #F5F6F7；③`Panel`/`CentralPanel` 无 `.fill()`，预览区用 `CentralPanel::frame(Frame::default().inner_margin(8).fill(content))`。
- **实测验证**（llvmpipe + import 截图 + 像素采样）：侧栏 #F0F3F5、菜单栏灰、编辑器 TextEdit 区白、全图 918 个像素命中 #3370FF（页签蓝条/侧栏选中/链接）；暗色一套同构投影。
- **如何改**：嫌蓝不对就改 `theme.rs::shell_tokens` 与 `ui/tokens.rs::accent` 两处（前后者管页签/选中，前者管面板底色）；AI 元素的紫罗兰在 `ui/preview.rs::ai_link_color`，要跟着改蓝就在那。

## #27 大纲预览跳转：为什么动了 vendor、以及滚动目标的归属（2026-09-26）

- **岔路**：roadmap 写「大纲预览跳转（复用 `section_to_token` 映射）」，但 vendored `MarkdownLabel` 把内容画进单个 galley，`section_to_token` 与布局 y 坐标都不对外暴露 —— app 侧无论怎么算都拿不到「这一节在第几像素」。可选：①动 vendor 暴露锚点；②app 侧按字节比例估算 y；③不做跳转。
- **自动选择**：**①动 vendor（①类，纯新增能力）** —— `SectionAnchor { byte_start, y }` + `section_anchors(ui, id)`，在 `render_galley` 两条分支记录各 section 顶部 y 到 `ui.data`。②不可接受：代码块、表格让「字节比例 → 像素」的误差大到能差好几屏，跳转就失去意义。
- **滚动目标算谁的状态**：**UI 关注点，不是文档状态** —— 存 `PreviewState::scroll_target`，由预览绘制消费一次（与侧边栏把手、键位捕获同一口径）。若走归约，则每帧都会重新滚动，用户再也滚不动预览。
- **已知并接受的边界**：`byte_start` 是**渲染文本**（经 wikilink 展开 / heal）内的偏移，而大纲 span 基于**源码**；文档里有 `[[wikilink]]` 时两者会错位（误差等于展开新增的字符数）。heal 对完整文档是恒等变换，故绝大多数文档不受影响。跳转粒度是**节**（section），不是精确的标题行 —— 落在标题所在节的顶部。
- **如何改**：要精确对齐，在 `expand_wikilinks` 同时产出「源码偏移 → 渲染偏移」的映射表，跳转前换算；要做到标题行级，让 vendored 记录每个 token 的 y 而非 section 的 y。

## #26 `[[wikilink]]` 的三处口径：展开位置、代码块豁免、匹配规则（2026-09-26）

- **岔路**：roadmap 只写了「`[[wikilink]]` 双向链接（LinkHandler）」，落地三处自由度。①`[[目标]]` 在哪一层变成可点击的链接（改源码 / 改解析 / 只改渲染）；②代码块里的 `[[…]]` 算不算链接（`arr[[0]]` 这种 Rust 代码会被误伤）；③「目标」怎么匹配到文件（精确文件名 / 去扩展名 / 大小写 / 子目录）。
- **自动选择**：①**只改渲染** —— `latermd_md::expand_wikilinks` 把 `[[目标]]` 展开成 `[显示名](<wiki://目标>)`，结果存进 `PreviewState::rendered`（随修订号重建，空闲帧不付代价），**源码一字不改**（roadmap P0 验收「`.md` 保持原样」）；点击由既有 LinkHandler 拦 `wiki://`（与 `ai://` 同一条通道），颜色取青绿与 AI 紫罗兰区分。②**围栏代码块内的 `[[…]]` 不算链接**（以 ``` / ~~~ 切换代码态，与 CommonMark 一致）。③匹配走 `filetree::find_by_name`：文件名去扩展名后**忽略大小写全等**、扩展名须 md/markdown；目标带 `/` 时按相对路径直取；遍历复用 `latermd_search`，因此同样尊重 `.gitignore`。
- **已知并接受的边界**：目标文档按**文件名**解析，不做「标题即文档」的别名解析（同一标题多篇文档会歧义）；超过 `MAX_LIST_ENTRIES`(500) 的库尾部分可能找不到（与搜索结果截断同语义，不谎称全库精确）；`[[目标|显示名]]` 支持显示名，但反向链接面板（谁引用了我）没做，属后续（2026-10-03 补记：已由 #15 BK1/BK2 落地，扫描与面板口径见 #87/#88）。
- **如何改**：要「标题即文档」，在 `find_by_name` 未命中时退回遍历各文件的首个 H1 做匹配（代价：全库读头，需缓存）；要反向链接面板，用 `latermd_md::wikilinks` 对每个文件扫一遍建索引（代价：库大时要后台线程 + 增量）。

## #25 Live Preview v1 的五处口径：块粒度、落点、路由、重算、持久化（2026-09-26）

- **岔路**：roadmap 阶段 5 只写了「光标所在 block 显示源码」+「v1 可简化为聚焦时整条源码裸出来」，落地时五处自由度。①块按什么粒度切（段落 / 行 / token span）；②点非活动块时光标落在哪（精确 hit-test 还是块末）；③↑↓ 跨块怎么走、Home/End 要不要接管；④编辑导致块分裂/合并后，活动块按序号记还是按光标记；⑤`render_mode` 要不要持久化。
- **自动选择**：①**块 = `latermd_md::blocks` 的字节区间**（段落按空行切、块级 token 独占、列表每项一块），且**连续覆盖全文** —— 这是安全底线：光标块是被编辑的区间，落在块外的字节会在敲键时静默丢失（单测 `assert_covers_text` 钉住）；②点击落点取**块末**（精确 hit-test 要反查文本布局，v1 不追求像素级精确）；③↑ 在块首去上一块**末尾**、↓ 在块尾去下一块**开头**，Home/End 仍交给 TextEdit 内建；④活动块**按光标字节重定位**（`LiveState::sync`）—— 按序号记必然错位：在第 2 块开头敲回车后原第 3 块变成第 4 块；⑤**不持久化**（会话内偏好，持久化需新增配置文件，收益小于成本）。
- **如何守住铁律**：活动块的编辑经 `BlockBuffer`（egui `TextBuffer` 适配）按「块内偏移 + 块首偏移」落回**同一个 `EditorBuffer`**，文本没有第二份真源；两种模式只是 `render_mode` 一个标志的分派，切换零搬运 —— 单测 `toggling_live_preview_only_flips_the_flag` 钉住（文本/修订号/dirty 都不变）。
- **已知并接受的边界**：块是**整块**切换，不做内联标记半隐藏（`**` 只隐藏一半那种），属 v2；每块的 undo 快照是本块的（跨块撤销按块分段，不是全文一步撤销）；MarkdownLabel 不返回响应，非活动块的点击区域用渲染前后的 cursor 差值框出来（近似区域）。
- **如何改**：要做像素级点击落点，用 `MarkdownLabel` 的布局信息反查字符偏移（需 vendor 侧暴露，属 ①类改动）；要全文统一 undo，把 undoer 从 TextEdit 内建换成自维护的（代价：IME 组合与选区行为要自己兜）；要 v2 内联半隐藏，在块内再按 token span 分段渲染（roadmap 已列为 v2）。

## #24 皮肤批次 B 的四处口径：System 解析、皮肤存储、密度基准、文件名（2026-09-26）

- **岔路**：roadmap「专题：界面美化与皮肤系统」批次 B 只写了「三态切换 + 自定义皮肤文件 + 视觉打磨」，落地时有四处自由度。①`跟随系统` 是个**非确定值**：检测结果缓存在哪、多久刷一次、检测不到怎么办；②皮肤文件用什么格式、存哪、内容要不要再存一份进 `settings.json`；③密度 token 怎么算（以出厂值为基准还是基于当前值缩放、动不动字号）；④皮肤名来自用户输入却要拼进路径。
- **自动选择**：①`ThemeMode::System` 只在 `resolve(detected, fallback)` 处落到确定值 —— 结果缓存在 `State::system_theme`，**仅跟随系统模式才轮询**（1 秒节流；非跟随模式返回 `None`，让 egui 收敛到深度空闲），检测失败与 `Unspecified` 一律回落上一次的手动值并在设置页明示「本机读不到系统主题设置」，不猜；②皮肤文件是**唯一事实源**：`themes/*.ron` 存 `MarkdownStyle`，`settings.json` 只存皮肤名，启动扫描目录把内容载入 `ThemeSettings::skin_style`（`#[serde(skip)]`）—— 避免同一份样式两处存放、改一处另一处不跟着变；用目录扫描而非配置清单，用户把别人给的 ron 丢进目录即生效；③密度以 `egui::Style::default()` 的出厂值为**基准**缩放（间距/控件尺寸 0.7、圆角 0.8、滚动条同比例收窄），不基于「当前值」再乘（否则标准↔紧凑来回切会逐次累积）；**不动字号** —— 中文在小字号下的可读性损失远大于多出来的几行；④皮肤名经 `skin_file_name` 清洗（`/ \ : * ? " < > | .` 全换 `_`，空名给默认名），挡住 `..` 拼进路径。
- **已知并接受的边界**：跟随系统的首次值来自启动那次探测，系统切主题后最迟 1 秒跟上；Linux 上 `dark-light` 走 freedesktop portal，Deepin/KDE 等环境可能恒返回 `Unspecified`（本机实测结果待人工补记）；皮肤只覆盖**正文与代码高亮**（`MarkdownStyle`），外壳配色仍由 egui 自带的 light/dark visuals 决定 —— 要让外壳一起换色需另加 shell token 表，属批次 C（明确不做）。
- **如何改**：要更快的系统主题响应，调小 `SYSTEM_THEME_POLL`（代价：更频繁查 dbus/注册表）；要让外壳也随皮肤换色，在 `ThemeSettings` 加 shell token 表并在 `apply_density` 同处投影；要让皮肤内容也进 `settings.json`（自包含），去掉 `skin_style` 的 `#[serde(skip)]` 并在 `select_skin` 里同步写回 `overrides`（代价：两份真源，改皮肤文件后界面不变）。

## #22 MCP server 落地的六处口径（2026-09-26）

- **岔路**：mcp-plan.md 给了形态与工具集，落地时仍有六处自由度。①HTTP 与 stdio 谁是主通道（GUI 进程内的 stdin 不是管道，stdio 在常驻进程里没有客户端）；②`tools/call` 缺 `name` 该怎么报错；③关掉的工具是「调了才拒」还是「对客户端不存在」；④文件树换根后服务要不要重启；⑤`--mcp-stdio` 子进程模式要不要受 `mcp.json` 的 `enabled` 约束；⑥`list_files` 的 glob 用什么实现（引 `glob` / `globset` 还是复用 `ignore`）。
- **自动选择**：①**HTTP 是 GUI 进程内的主通道**（应用开着就能被调 —— 坤哥的诉求原话），stdio 作为 `--mcp-stdio` 子进程模式给 `claude mcp add` 这类客户端，两者共用同一个 `Server`，只是传输不同；②缺 `name` 走**协议层 `InvalidParams`**（-32602），工具执行失败才走 `isError` 内容块 —— 前者是请求格式问题、后者是工具结果，混在一起客户端不好分支；③关掉的工具**不出现在 `tools/list`**（最小权限要真的生效，而不是「列出来让你调、调了才拒」），真被点名时仍回「工具已在设置里关闭」；④换根走 **`SharedRoot` 共享句柄**（`Arc<Mutex<Option<PathBuf>>>`），服务不重启；⑤headless 模式**不看 `enabled`** —— 用户显式用参数启动就是一次授权，而 `enabled` 管的是「GUI 进程内是否自动监听端口」这件不同的事；根取环境变量 `LATERMD_MCP_ROOT`；⑥glob 走 **`ignore` 的 override 匹配查询**（`Override::matched(path, is_dir)`）而非 `builder.overrides()` —— 实测后者只筛文件、目录条目照旧产出（`*.txt` 会带出 `notes` 目录），而列目录的语义是「条目本身要不要出现」，目录必须过同一把筛子；零新增依赖。
- **已知并接受的边界**：HTTP 侧**单连接串行**（工具是毫秒级检索，排队即可，也避开「两个 AI 并发改同一个编辑器缓冲」）；不实现 MCP 的 `resources` / `prompts` / `sampling` 与 SSE 长连接流（客户端要 SSE 时按单帧 `data:` 回，语义与 JSON 一致）；`outline` 的行号按标题 span 换算，而 span 会吸收上一块尾部的换行（latermd-md 的已知行为），换算时跳过前导换行。
- **如何改**：要 stdio 当主通道，把 GUI 启动的 `http::serve` 换成 `stdio::serve`（代价：常驻 GUI 的 stdin 无处接客户端，等于放弃「应用开着就能被调」）；要让关掉的工具仍出现在列表里，去掉 `tool_list` 的 `filter`、保留 `tool_call` 的拒绝分支；要并发 HTTP，把 `serve_with` 的 accept 循环改成每连接一个线程（需同步处理工具对同一库的并发读）。

## #23 多标签归约迁移的三处消息口径:TabOpen 不引入、确认关闭不带载荷、孤立 chunk 丢弃(2026-09-26)

- **岔路**:multi-tabs 棒的任务规格写「Message 新增 `TabOpen { path }`、`TabCloseConfirmed { index }`」,而 main 上已落地的多标签骨架(`crates/latermd-app/src/tabs.rs` + state.rs 归约)用了不同的等价结构;另有一个规格没覆盖的防御分支(在途流的发起标签已不存在时,迟到的 `AiChunk` 写到哪)需要定口径。
- **自动选择**:①**不引入 `TabOpen`**——「打开」的三个入口(菜单「打开」对话框 / 文件树点击 / 搜索跳转)都在归约内部完成「路径去重 → 命中激活 / 未命中开新标签」(`State::open_path`),UI 层没有任何场景需要直接产出 `TabOpen`,引入无人产出的消息只增表面积;②**`TabCloseConfirmed` 不带 `{ index }` 载荷**——确认目标存在 `TabsState::confirm_close`(请求时刻的快照),确认的必是弹窗所问的那个标签,比消息载荷更防错(载荷在模态期间标签增删后会指错对象);③**孤立 chunk 丢弃并作废流**(`State::append_ai_delta` 的 `ai_stream_tab_index() == None` 分支)——发起标签被关闭时 `remove_tab` 已先行作废流,真实链路走不到该分支;万一未来重构弄丢绑定,fail-safe 是「丢块可见(续写中断)」而非「静默写进 active(写错文档)」。
- **如何改**:要让 UI 能直接开标签(比如将来的拖拽打开),加 `Message::TabOpen { path: Option<PathBuf> }` 并在归约里转 `open_path`/`spawn_tab` 即可;要确认关闭改带载荷,给 `TabCloseConfirmed` 加 `usize` 并在 `layout.rs` 的 `tab_close_dialog` 处带上 `confirm_close` 的值;要孤立 chunk 落到当前标签,把 `append_ai_delta` 的 `else` 分支改成 `self.tabs.current_mut()`(须接受写错标签的风险,不建议)。

## #22 界面打磨批次的四个口径:图标自绘、撞键拒绝、未实现项禁用、MCP 只出规划(2026-09-25)

- **岔路**:用户指令「图标、快捷键设置、AI 配置页、MCP 规划」留了四处自由度。①egui 无图标集,用 emoji/Unicode 字符(✎ 🗋)还是自绘?②改键撞到别的命令的键位时,抢占还是拒绝?③AI 配置页的「接口方式」里 Anthropic/Ollama adapter 还没写,下拉里给不给选?④MCP 做到什么深度?
- **自动选择**:①**全部自绘**(`ui/icons.rs`,`Painter` 线段/圆/矩形,归一化坐标)—— emoji 在三平台缺字风险真实(AGENTS §5 已把字体列风险项),自绘零字体/纹理依赖且随主题取色;②**拒绝并指名占用者**("Ctrl+K 已被「打开」占用,未修改")—— 静默抢占会让用户莫名丢另一个命令的键位;裸字母/数字一律拒绑(会被编辑器当输入吞掉);③**显式禁用并写明"未实现"**—— 伪装可选会让用户配完发现没生效,与 decisions-pending #12 同口径;④**只出规划文档**([mcp-plan.md](mcp-plan.md))与设置页禁用态开关,不写半截 server —— MCP 是新增范围(AGENTS §7 深水区之外),该有单独立项,设置页伪造"运行中"不可接受。
- **如何改**:要换字体图标方案,`ui/icons.rs` 的 `Icon::draw` 是唯一绘制点;要改抢占语义,`State::assign_shortcut` 的 conflict 分支改 `set` 即可;要实现 Anthropic/Ollama,在 `latermd-ai` 加 adapter 并放开 `settings.rs` 里 `implemented()` 的两个禁用点;MCP 开工按 mcp-plan.md 的阶段表走。

## #21 设置面板 AI key 接线的三岔路：浮窗形态、状态分组、key 闸门位置（2026-09-25）

- **岔路**：任务写「Settings 面板新增 AI Provider 区」，但仓库没有独立 Settings 面板实体——设置只有工具栏的「设置」`menu_button`（`ui/toolbar.rs`），且仓库自己的注释证明「egui 菜单内点击任意控件自动收起」，把密码框 TextEdit 直接嵌进菜单有「点进输入框菜单即收起」的交互风险；任务又写「State 增加 `ai_key_configured: bool`」，字面平铺与仓库的状态分组风格（`SidebarState`/`SearchState`/`AiState`）相悖；key 闸门（provider 启动链路）若放流式共用入口 `start_ai_stream_with_prompt`，`AiStart` 的外层归约会先补空行、`AiSummaryRequested` 会先移除旧摘要节——被拦的命令留下副作用。
- **自动选择**：①「设置」菜单加「AI Provider…」入口（原地翻转 `dialog_open`，同 `SidebarState::visible` 的 UI 关注点口径），密码框/保存/清除/状态行放独立 Window 浮窗（与 commit 建议浮窗、回滚确认浮窗同模式，`crates/latermd-app/src/ai_key.rs::ai_key_dialog`）；②状态分组成 `State.ai_key: AiKeyState`（`configured`/`backend_ok`/`draft` 在内），任务字段的语义落点 = `state.ai_key.configured`；③key 闸门放在**每个 AI 命令归约的最前面**（`AiStart`/`AiLinkClicked`/`AiCommitRequested`/`AiSummaryRequested` 四入口，模式 `if self.ai.is_streaming() || !self.ai_key_gate() { return; }`），被拦命令零副作用；provider 是否需 key 由 `AiState::provider_requires_key` 表达，当前 Mock 恒 `false` 直通（无 key 也能跑），主模型启用时随 provider 置 `true` 即生效——测试已覆盖置 `true` 后的拦截/放行两分支。
- **如何改**：要密码框直接长在菜单里，把控件从浮窗搬进 `menu_button` 闭包并实测菜单收起行为；要平铺字段就把 `AiKeyState` 拆散上提到 `State`；要 Mock 也强制配 key，把 `provider_requires_key` 默认值改 `true`（测试 `ai_stream_blocked_without_key_when_provider_requires_it`/`mock_provider_runs_without_key` 同步改）。

## #20 latermd-creds 的四岔路：keyring 维护线、get_secret 签名、env 回退测试注入、测试值口径（2026-09-25）

- **岔路**：P2 凭据 crate 落地时任务留了四处自由度。①keyring crate 有两条版本线：3.6.3（hwchen 原维护线终版，无默认 features，需手工配平台组合，已随项目移交停更）与 4.2.0（open-source-cooperative 接管后的重构线，2026-08 仍更新，默认 feature `v1` 即三平台 store）；②任务签名写作 `get_secret(...) -> Option<String>`，但同批约束要求「所有后端调用优雅降级（Err 返回）」且单测要「断言错误文案不含 secret」——Option 装不下错误文案；③环境变量回退顺序的测试：临时改进程环境变量（并行测试竞态）还是注入；④红线「凭据值不进测试断言明文、测试只断言存在性/删除成功」与「内存后端全 CRUD」的关系——CRUD 的 R 不验证读回值就测不出后端正确性。
- **自动选择**：①**keyring 4.2.0**：4.x 是唯一仍在维护的线；默认 feature 按 target 自动落 Windows Credential Manager / macOS Keychain / Linux Secret Service（zbus 纯 Rust 实现，不链 libsecret C 库；Cargo.lock 既有 zbus 条目复用）；keyring-core `Error` 的 `Display` 实测不携带凭据字节（`BadEncoding` 打固定文案）；②`get_secret` 返回 `Result<Option<String>, CredentialError>`（错误可见、文案可断言、不吞「后端坏了」），`has_secret -> bool` 与 `ai_api_key -> Option<String>` 保持任务签名——Err 折叠为 false/None 的降级语义自洽（便捷查询定位，用户重新保存时会看到 set 的真实错误）；③注入式：`Credentials::ai_api_key_from(env_value: Option<&str>)` 显式传环境变量取值（测试注入点），顶层 `ai_api_key()` 内部读真环境变量；④测试值全部是 `placeholder-*` 占位假值，断言只做相等性/存在性比较——验证的是后端读写一致性，不是把真实凭据写进断言。
- **已知并接受的边界**：后端读失败时 `has_secret` 返回 false（「不可用」与「未配置」在便捷查询层不可区分）；keyring 4.x 的 v1 模块在 Linux 无 dbus 时首次 `Entry::new` 即快速失败并**缓存**初始化结果——优雅降级成立，但运行中途 Secret Service 才挂掉的场景不会重试（LaterMD 桌面应用的 keyring 在进程启动后基本常驻，可接受）。
- **如何改**：要回 3.x 线，把 crate Cargo.toml 改 `keyring = "3.6.3"` + `features = ["apple-native", "windows-native", "sync-secret-service"]`（`NoEntry` 匹配同款，改动很小）；要 `get_secret` 恢复纯 Option 签名，删 `Result` 包装并把「错误文案不含 secret」断言收缩到 set/delete；要改用进程级环境变量测试，删 `ai_api_key_from` 注入点、测试里 `std::env::set_var`（须接受竞态或串行化）；要把「存在性-only」测试口径执行得更严，删 CRUD 测试里的相等性断言（代价：后端写坏值不再被测出，不建议）。

## #19 git status 的条数上限与超限文件的行为（2026-09-25）

- **岔路**：独立评审指出 `latermd_git::status` 无条数上限（`recurse_untracked_dirs` 全量展开），叠加同步跑在 UI 线程的 3s 轮询与侧边栏每帧全量渲染，超大仓库会卡帧——log（50 条）与 diff（64KB）都有上限，唯独 status 没有。加多少、超限文件的行为（角标/选中/回滚）怎么定未指定。
- **自动选择**：上限 **500**（`latermd_git::DEFAULT_STATUS_LIMIT`），与文件树 `MAX_CHILDREN`、搜索 `MAX_HITS` 两个既有先例同量级；`status(root, limit)` 返回 `StatusSnapshot { entries, truncated }`，**先按路径排序再截断**（保留字典序最小的前 500 条，保证确定性）；Git 页改动列表尾部渲染「…还有 N 项未显示」（与文件树同款提示行）。超限文件的降级：无文件树角标、不可选中/回滚（select/request_checkout 的「列表外路径忽略」防御天然覆盖）——与文件树截断、搜索 MAX_HITS 同语义。注意：libgit2 的 statuses 遍历本身无法提前截断（StatusOptions 无 limit），本上限消除的是「Vec 无界 + 每帧全量渲染」两项；遍历成本仍属 #17 已登记的「掉帧再挪线程」取舍。
- **如何改**：嫌 500 太小改 `DEFAULT_STATUS_LIMIT` 一个常量（调用方 `git_panel.rs` 自动跟随）；要「显示全部」，给 Git 页加展开交互并让 `status` 支持分页或提高上限；要消除遍历成本，把 `recurse_untracked_dirs` 关掉（未跟踪目录只报目录一条，快得多，但文件树逐文件打标失效）或按 #17 的「如何改」挪后台线程。

## #18 回滚目标恰是编辑器当前文档时，dirty 缓冲的处置（2026-09-25）

- **岔路**：独立评审指出回滚（checkout 单文件）若目标正是编辑器当前打开的文档，归约后编辑器不重载、无提示——dirty 场景一次 Ctrl+S 就把被丢弃的改动静默写回（反转回滚）；非 dirty 场景编辑器显示与磁盘不一致。修法有两派：①回滚后无条件重载编辑器（强一致，但 dirty 时静默丢掉未保存稿）；②分 dirty 分流。
- **自动选择**：**②分 dirty 分流**（`State::after_git_checkout`）：目标非当前文档不触碰编辑器；是当前文档且非 dirty → 重读磁盘换入缓冲（预览同帧联动，编辑器与磁盘重新一致）；是当前文档且 dirty → **保留未保存稿**（静默丢稿的代价大于不一致，与 `unsaved_guard` 的既有哲学一致），提示行明示「已回滚 X：编辑器里未保存的修改仍保留，保存(Ctrl+S)会把它们写回」。确认模态同步加针对性警示（`checkout_dialog` 的 `checkout_extra_warning`）：目标在编辑器中打开时按 dirty 显式告知上述行为，不再只有通用不可逆警示。
- **如何改**：要①的强一致语义，把 `after_git_checkout` 的 dirty 分支改成同样调 `open_from(file)`（并在 `checkout_extra_warning` 的 dirty 文案里说明将丢弃编辑器修改）；要更保守的「dirty 时拒绝回滚」（像 unsaved_guard 那样拦下），在 `request_checkout` 前置检查并落提示行。

## #17 Git UI 接驳的刷新机制、仓库根定位与确认模态形态（2026-09-25）

- **岔路**：把 latermd-git 接进侧边栏时任务留了三处自由度。①「每 N 秒或触发时刷新」的 N 未定，且同步归约执行还是后台线程未定（`git status` 大仓库冷缓存可能上百毫秒，卡帧风险真实存在）；②#16 ③ 已定 latermd-git API 层用 `Repository::open` 严格仓库根，但 UI 的文件树根常是**仓库子目录**（比如选了 `docs/` 当根），以哪个目录调 status/diff/log 没定；③「确认模态」在 egui 0.36 没有内建阻塞模态层，用什么形态承载。
- **自动选择**：①**N=3 秒，同步在归约里执行**（`crates/latermd-app/src/git_panel.rs::REFRESH_INTERVAL`）：与 `git_diff.rs` 的既有口径一致（本地 git 读是毫秒级，不上后台线程），到点由 `ui::layout::reduce` 触发并 `request_repaint_after` 要帧；触发式刷新 = 换根 / 切到 Git 页 / 回滚完成；**降级（非 git 目录）即停轮询**，重探由换根/切页签触发——保证 egui 空闲收敛（search 去抖测试守护的不变量），零成本挂着的失败探测没有价值。②新增 `latermd_git::discover`（`Repository::discover` 向上探测，裸仓库显式 Err）：crate 其余 API 的「严格仓库根」口径不动，UI 接驳层先 discover 把文件树根换算成仓库根；状态条目仍记「相对仓库根」路径，角标拼成绝对路径与文件树条目匹配。③确认模态用**非阻塞 `egui::Window` 浮窗 + 红色警示文案「未提交的改动将被丢弃，此操作不可撤销」**（与 AI commit 建议浮窗同模式）：checkout 只在「回滚」按钮点击后的消息归约里执行，浮窗本身零 git 调用。
- **如何改**：嫌 3s 太钝/太勤，改 `REFRESH_INTERVAL` 一个常量；大仓库实测掉帧，把 `GitPanelState::refresh` 的两次 git 读挪后台线程（对 UI 的接口不变，参照 search 的代际号取消模式）；要收紧回「文件树根必须是仓库根」，删 `latermd_git::discover` 并让 `GitPanelState::refresh` 直接以文件树根调 status（非根目录会走降级提示）；要真阻塞式模态，等 egui 内建 modal 层（0.36 无）或自绘全屏遮罩 Area。

## #16 latermd-git 的三个落地口径：U 的语义、git2 features、仓库根定位（2026-09-25）

- **岔路**：P2 首个 Git crate 落地时任务留了三处歧义。①状态码集合写作 `M|A|U|D|?`，U 是 unmerged（git CLI short format 语义）还是 untracked（VS Code 装饰字母语义）——若 U=untracked 则 `?` 无含义。②ADR-004 登记 git2 0.21.0 的组合是 `vendored-libgit2 + vendored-openssl`，但 vendored-openssl 会拉 openssl-src 全量编译（三平台 CI 各多数分钟），而它的唯一用途是 https 传输。③API 以仓库路径为参数：`Repository::open`（严格根）还是 `Repository::discover`（向上层搜 `.git`）。
- **自动选择**：①**U=unmerged（合并冲突），?=untracked**，按 git CLI `--short` 语义（`crates/latermd-git/src/lib.rs::StatusKind`），与 roadmap「文件树 Git 标记 M/A/U/?」并排五码自洽；②git2 取 **`default-features = false, features = ["vendored-libgit2"]`**（版本 0.21 与 ADR-004 一致）：P2 明确只读、无 fetch/push/pull，ssh/https 传输层整层用不上，关掉后零 openssl 面（不依赖系统包、不编译 openssl-src）；若将来做 remote 再加 `vendored-openssl` 即可；③**`Repository::open` 严格仓库根**，不向上搜——与 #14 app 侧「不向上搜 `.git`」的既有口径一致，非 git 目录一律 `Err` 交给 UI 降级成提示。
- **已知并接受的边界**：status 里非 UTF-8 文件名不出现（libgit2 的 `entry.path()` 返回 `Option<&str>`，非 UTF-8 时为 None，极罕见）；blame 基于 HEAD 提交内容，工作区未提交的行不参与行级归属（libgit2 限制，doc 已注明）；`checkout_file` 的 path 走 git pathspec 语义（与 `git checkout -- <path>` 一致，含 glob 元字符的文件名理论上可被通配匹配）。
- **如何改**：要改 U=untracked，改 `lib.rs::status_kind` 的优先级映射一处（untracked 同时映射 U 与 `?` 的需求不存在，五码本来就单字母）；要恢复 ADR 原样的 openssl 组合，把 crate Cargo.toml 的 features 改回 `["vendored-libgit2", "vendored-openssl"]`（须同时去掉 `default-features = false`，否则 https feature 仍关着）；要支持从子目录自动定位仓库根，把 `open_repo` 的 `Repository::open` 换成 `Repository::discover`，但 status/diff 的相对路径语义需随之在 UI 侧重排。

## #15 AI 摘要的插入形态、引用块前缀来源与 Mock 请求识别（2026-09-25）

- **岔路**：任务把展示形态留成二选一——「以引用块形式插入文档末尾」或「展示给用户可选插入」。另外 prompt 输出要求固定为「每条一行中文，以 '- ' 开头」，而最终插入形态是引用块（`> - …` 行），`> ` 前缀由谁加上、Mock provider 在共用 `stream_complete` 通道里如何区分摘要请求与续写请求，都需要定口径。
- **自动选择**：**直接插入文档末尾**（`crates/latermd-app/src/state.rs::request_summary`）。理由：摘要流式落文档与续写同语义，天然复用 `AiChunk` 追加通道与防重入；浮窗形态走不了流式追加（commit 选浮窗是因为 subject 是「建议」，摘要是要写进文档的内容）。`> ` 前缀由 **Mock 替身直接产出最终文档形态**（`crates/latermd-ai/src/mock.rs::mock_summary` 输出 `> - …` 行序列），prompt 指令保持任务原口径不改；真实 key 接入时在适配层把模型输出的 `- ` 行包成 `> - `（与 commit 的真实 provider TODO 同批，见 `request_commit_message` 的 TODO 注释）。Mock 请求识别用**摘要指令头前缀嗅探**（`stream_complete` 里 `starts_with(SUMMARY_INSTRUCTIONS)`，与现有「按 prompt 关键词选脚本」同构；真实 provider 无此问题，模型自己读指令）。
- **已知并接受的边界**：①摘要节定位按「二级标题 + 文本精确等于 `AI 摘要`」（`latermd_md::heading_section_span`），用户改层级/改名后的旧节不清理（保守匹配，防误删手写内容）；②流失败时旧节已删、新标题已插、要点可能半截——与续写流「失败留半截正文」同语义，演示期 Mock 不产生失败块；③摘要节被用户挪到文档中间且其后还有内容时，删节后正文与下一节间保留一个换行（合法 Markdown，源码视觉紧凑）。
- **如何改**：要改浮窗形态，在 `State` 加 `ai_summary_suggestion: Option<String>` 并把 `request_summary` 改成收流进缓冲区外的暂存（AiChunk 归约需按流类型分流）；要匹配用户变体的旧节，放宽 `heading_section_span` 的层级参数或做模糊文本匹配；要让真实 provider 输出自动加 `> `，在接入 `OpenAiProvider` 时于 app 侧对摘要流的 chunk 做行级包装（需在 `AiState` 加当前流类型标志）。

## #14 AI commit message 的仓库定位与浮窗/复制口径（2026-09-25）

- **岔路**：菜单「AI: 生成 commit message」要读「当前仓库」的未提交改动，但 LaterMD 没有「当前仓库」的概念——文档可以不在任何 git 仓库里，文件树根也可以是任意目录，还可以向上搜 `.git` 找仓库根。另外 ask 把展示（状态栏 vs 对话框）与复制（按钮 vs 自动写剪贴板）留成二选一。
- **自动选择**：仓库定位取**当前文档所在目录，退文件树根目录**，两者皆无则提示「先保存文档或设置文件树根目录」；不在仓库/无 git 时 `git diff` 的 stderr 直接落提示行（`crates/latermd-app/src/state.rs::request_commit_message`）。不向上搜 `.git` 找根：`git diff` 在仓库子目录里跑也返回全仓改动，先找根纯属多余。展示用**浮窗对话框**（subject 要整行可读，状态栏 notice 行是错误专用、红色语义不符）；复制用**显式「复制」按钮**（`Context::copy_text`），不自动写剪贴板——未经用户动作覆盖系统剪贴板会冲掉用户正在搬运的内容。生成路径演示期为 `MockProvider::mock_commit_subject` 同步合成（流式脚本对 commit 场景不适用），真实 key 接入后改走 `OpenAiProvider` 低温度补全取首行（同函数内的 TODO）。
- **如何改**：要支持显式指定仓库（如设置面板里选仓库根），改 `request_commit_message` 的目录解析一处即可；要改自动复制，在 `Message::AiCommitSuggestion` 归约里补 `Context::copy_text(subject)`（消息已带 subject，归约侧拿得到 ctx）；要让建议随换文档消失，在 `State::load_document` 里顺手清 `ai_commit_suggestion`（本轮刻意不清：建议是仓库级派生物，不是文档的）。

## #13 ```ai 指令卡状态行的键控口径（2026-09-25）

- **岔路**：指令卡状态行要求「未执行 / 进行中 / 已完成」三态，需要回答「哪张卡算进行中/已完成」。可选：①按卡片指令文本与最近一次发起的 prompt 匹配（`AiState::last_prompt`）；②按块在文档中的序号维护每卡状态表。
- **自动选择**：①（`crates/latermd-app/src/ui/preview.rs::AiLinkHandler::card_status`）。理由：防重入保证同时至多一个流，「哪张卡发起」由 prompt 文本即足以判定；序号表在用户增删块时会整体错位，还要处理失效清理；文本匹配零新增结构。已知并接受的简化：**两卡片指令文本完全相同则状态同亮**；指令文本被编辑后状态回「未执行」（文本变了=另一条指令，语义自洽）。菜单入口（`AiStart`）的 prompt 是文档尾部拼装文本，不会与任何指令文本相等，菜单流不点亮卡片。
- **失效时机**：`last_prompt` 只在两处清空——流失败（`Message::AiFailed` 归约，失败不算完成）与换文档（`State::load_document`，卡片是文档的派生物）；`AiDone` 后保留，让「已完成」可见。
- **如何改**：要按序号键控（同文卡片状态独立），在 `PreviewState` 加 `card_status: HashMap<usize, AiCardStatus>` 并把 `block_code_widget` 的 `index` 传进消息，即可替换匹配逻辑；消息归约与 vendored 扩展点无需动。

## #12 ```ai 指令块走最小 vendor 改动（代码块级 block widget 扩展点）（2026-09-25）

- **岔路**：任务优先「只用 app 侧扩展点，不动 vendor」。实测 vendored `LinkHandler::is_block_widget`/`block_widget`（`vendor/egui_markdown/src/link.rs`）只作用于 **`Token::Link` 的 href**（判定点 `layout.rs` `append_link_to_job`/`needs_segmentation`/`build_layout`），**够不到围栏代码块**——roadmap 阶段 3 写的「`.is_block_widget()` → `.block_widget()`」对 ```ai 围栏不成立。app 侧唯一代码块扩展点是 `code_block_buttons` 头部 overlay 回调（回调签名 `(ui, text, lang)`，无块序号/span），画不出「卡片 + 状态行」，也拿不到稳定块身份（AGENTS §6.7 的 id 稳定性无从谈起）。
- **自动选择**：给 vendored `LinkHandler` 加**代码块级 block widget** 两方法（`is_block_code_widget(language)` / `block_code_widget(ui, text, language)`，按 info string 判定），与链接 block widget 同构：命中即 segment break，在 `render_token_range` 独立渲染；`needs_segmentation` / `build_layout` / `render_token_range` 三处按上游既有「必须同步」约定同步改。类别 **①上游可合**（通用能力、带 tests/block_code_widget.rs，可 cherry-pick 提上游 PR），登记见 `vendor/README.md` 提交级登记表与 `vendor/egui_markdown/README.md` 差异表 #7。
- **如何改**：若不认可动 vendor，revert 该 ① 类 commit 并把 app 侧退到 `code_block_buttons` overlay 形态（功能降级：状态行并入代码块头、卡片视觉消失、多卡身份按内容 hash 近似）——代价已实测如上，不建议。

## #11 `ai://` 链接协议语义与 prompt 编解码口径（2026-09-25）

- **岔路**：roadmap 阶段 3 对「ai:// 链接协议」只写了「`.link_style()` + `.click()` 拦截」的实现方式，协议本体没有定稿——已实现动作是哪个、未实现动作点了怎么办、prompt 怎么编码、`+` 算不算空格，都得有个说法才能写测试。另实测发现 vendored 的 `LinkStyle.underline` 字段（`vendor/egui_markdown/src/link.rs:86`）当前**没有任何读取点**。
- **定稿**：`ai://write?prompt=<urlencoded 提示词>` 触发 Mock 流式续写，prompt **原样透传** provider（不拼文档尾部——链接作者写的就是完整指令；续写仍落在当前文档末尾，防重入与菜单「AI 续写」同一入口 `start_ai_stream_with_prompt`）。其余 `ai://` 动作（`ai://summarize` 等）**识别但不拦截成执行**：点击提示「未实现的 AI 动作：<action>」。`ai://write` 缺 prompt 参数、prompt 为空、坏 `%` 序列、非 UTF-8 字节，均提示且不执行。解码只用严格 `%XX`（`percent-encoding` 2.3.2，坏序列自行校验补严——该 crate 默认原样放行），**`+` 不当空格**：markdown 链接里作者本就该用 `%20`。非 `ai://` 前缀完全不拦截，走 vendored 默认 `open_url`（系统浏览器）。
- **证据**：vendored `layout.rs:144`（`link_style().color` 决定链接**文字色**）、`layout.rs:197`（hover 下划线对**全部**链接无条件绘制，颜色取 `link_style().color`）、`label.rs:1165`（`click` 返回 true 则跳过 `open_url`）。app 侧 `LinkStyle { color, underline: true }` 里 `underline` 是**声明意图**——vendored 层没人读它，样式区分实际由颜色承担；ai:// 链接取紫罗兰色（明暗主题两档），与默认 `hyperlink_color` 区分。
- **如何改**：新增动作或让 `+` 当空格，改 `crates/latermd-app/src/ai_link.rs::parse` 一处（消息载荷 `Message::AiLinkClicked { prompt: Result<String, String> }` 不变，`Err` 文案在 parse 里拼）；要让 `underline` 字段真正生效需改 vendored 层（①上游可合类），本轮按「优先只用 app 侧扩展点」未动 vendor。

## #10 heal() 的作用时机：逐块(后台线程) vs 整篇(渲染帧)（2026-09-25）

- **岔路**：AI 流式接线 ask 要求「后台线程每块先过 vendored heal()，经 mpsc 回 UI，delta 追加进编辑器 rope」。但 `heal(s)` 的语义是给**残缺文本前缀**补闭合标记（`vendor/egui_markdown/src/parser.rs:45`：`heal("```rust\nlet x = 1;")` → 追加闭合 fence、`heal("**bold text")` → 追加 `**`）。MockProvider 按固定 20 字符切块，块边界会切在代码 fence/加粗中间：若把逐块 healed 文本追加进 rope，闭合标记会被**永久写进文档**，且下一块拼在闭合 fence 之后产生非法残文（例：块尾 `fn invalidate(cache: &mut C` 被补成 `…C\n```` ，下一块 `ache, doc_id…` 紧跟其后再开一个 fence）。
- **自动选择**：heal 移到**渲染帧**、作用于**整篇快照**——预览 `MarkdownLabel` 开 `.heal(true)`（`crates/latermd-app/src/ui/preview.rs`），vendored 层在 parse 前对全文调 `parser::heal`（`label.rs` render 的既定钩子，docstring 即「Useful for streaming LLM output」）；编辑器 rope 只收原始 delta，文档内容不被污染。这与 AGENTS.md §6.5「让 LLM 流式输出的每一帧语法合法」一致——每一帧 = 每次渲染的全文快照。预览对完整文档 heal 是恒等变换（`Cow::Borrowed` 原样返回），非流式场景行为不变。
- **如何改**：若确实要逐块 heal（例如想把「 healed 帧」单独喂给某个纯预览通道、不进编辑器），在 `crates/latermd-app/src/ai.rs` 的 `poll()` 里对 delta 调 `egui_markdown::heal` 并另开一条不落盘的预览通道即可；只要别把 healed 文本写进 `EditorBuffer`。

## #9 流式失败信号与 OpenAI adapter 默认值（2026-09-25）

- **岔路**：`latermd-ai` 的 `Chunk` 按 ask 固定为 `{ delta, done }` 两字段，但流式请求失败（HTTP 非 2xx / 连接中断 / 读超时）没有天然的信号位；另外 OpenAI adapter 的默认端点、模型名与是否引入额外环境变量，ask 未规定。
- **自动选择**：约定「`done == true` 且 `delta` 非空 = 流失败，`delta` 是面向用户的错误描述（不写入文档）；`done == true` 且 `delta` 为空 = 成功结束」，见 `crates/latermd-ai/src/lib.rs` 的 `Chunk` 文档。默认端点 `https://api.openai.com/v1`、默认模型 `gpt-4o-mini`，可分别用 `LATERMD_AI_BASE_URL`、`LATERMD_AI_MODEL` 覆盖（key 仍只有 `LATERMD_AI_API_KEY`，decisions-pending #3 不变）。
- **如何改**：若希望失败信号更显式（如 `Chunk` 加 `error` 字段或改 enum），改动点集中在 `latermd-ai` 的 `Chunk` 定义与 `openai.rs::run`，消费方尚只有 mock 联调链路，无迁移负担；默认模型/端点改 `OpenAiProvider` 两个 `DEFAULT_*` 常量即可。

## #8 搜索去抖到点发起从 `ui::sidebar` 挪进归约侧（2026-09-25）

- **岔路**：修复「清空搜索输入 / 输入后切走页签后 `debounce_due` 残留过期时刻，`layout.rs` 每帧 `request_repaint_after(ZERO)` 满帧空转」时，评审给了两个薄修：①去掉 `ui::sidebar` 到点判断里的非空输入条件；②`layout.rs` 对已过期的 due 不再要帧。①只修「清空输入」主路径，「输入后切到 Files/Outline 页签」路径 `search_panel` 不渲染、无人清计时，依旧空转；②会打断接力最后一环——到点帧 reduce 先于 ui 执行，reduce 见 remaining==0 不要帧后，同帧 `ui::sidebar` 发出的 `SearchRequested` 滞留 outbox，无下一帧 apply，表现为「输入完不动鼠标搜索永不发起」。
- **自动选择**：把到点判断整体挪进 `layout.rs` 的 `reduce`（每帧必跑、不看页签可见性），到点当帧 `apply(SearchRequested)` → `SearchState::start` 入口清计时，过期 due 活不过一帧；重绘驱动与到点判断同处一处，两类残留（空输入 / 切页签）一并消除。副产品：输入后 300ms 内切走页签也照常发起，切回来即见结果（比「切回 Search 页才自愈」更符合直觉）。
- **如何改**：若更在意「只在 Search 页可见时才发起」，把 reduce 里的到点块移回 `ui::sidebar::search_panel` 并同时采纳②之外的方案（例如 reduce 里对过期 due 保留一次要帧兜底）；三个测试锚点在 `layout.rs`（`search_debounce_fires_in_reduce_even_when_tab_switched_away`、`search_debounce_due_cleared_for_empty_query_without_repaint_loop`）。

## #7 搜索核心不用 `grep-regex` 桥接（2026-09-25）

- **岔路**：roadmap 阶段 3 搜索条目的组件清单写作「`grep_searcher::Searcher` + `regex`」，但 `grep_searcher::Searcher::search_*` 全系 API 要求 `grep_matcher::Matcher` 实参，`regex::Regex` 并未实现该 trait，二者**无法直连**。要么补引 `grep-regex`（ripgrep 官方桥，连带 `grep-matcher`），要么改用同 crate 的 `grep_searcher::LineIter` 做行迭代、`regex` 直接匹配。
- **自动选择**：`LineIter`（`regex::bytes` 变体）+ 直接匹配，P1 搜索核心已按此落地（`crates/latermd-app/src/search.rs`）。不引 `grep-regex` 的理由：它带来的只是 trait 适配与流式读取，而 md 单文件整读进内存完全可行（实现里加了 16MB 单文件上限防病态大文件），大小写开关由 `RegexBuilder::case_insensitive` 一行承接；少一个清单外依赖比「与 ripgrep 同构」更有价值（#4 口径）。
- **如何改**：若 P1 搜索面板需要 multiline 正则或超大文件流式匹配，改引 `grep-regex` + `grep-matcher` 并在 ADR-004 补登，替换 `src/search.rs` 的行循环即可；对外三原语（发起 / 接收 / 取消）与事件模型不变。

## #6 双会话并发冲突（已裁决，2026-09-25）

- **事实**：2026-09-25 上午，本自动循环与另一活跃会话（UI 设计文档线）共享同一工作目录，互相踩踏致第一棒四次卡在 `git checkout main`，被主动停止（stop_reason=model，可恢复，无半截污染）。
- **裁决**：用户选择**选项 1（继续循环）**——已人工解决冲突（`d5de607` 取 stash 侧）、合并 origin/main 进 feature/kun、提交循环文档（`6b2ddc0`，已 cherry-pick 到 main 为 `8a500f0`）、清空 stash。
- **遗留提醒**：若其他会话今后仍需在此工作区工作，建议改用选项 2（`git worktree add ../LaterMD-auto main` + 脚本改造），循环脚本开头的两步 stash 防御只是兜底不是根治。

## #1 打包工作的合并（已解决）

- **原岔路**：打包工作在 `feature/p0-packaging` 分支，自动循环是否代为合并。
- **结果**：另一会话已走正规 PR 流程合并——PR #7（`feature/p0-packaging`：cargo-dist、universal2 dmg、release workflow、cask 模板）与 PR #8（`feature/m0-perf-bench`：长文档 bench）均已合入 main（`f822b61`）。剩余真机验收（打 tag 看 Release、brew 装机）仍属人工。

## #2 直推 main 豁免（2026-09-25 中午起已被现实推翻，改为 PR 自合并通道）

- **原自动选择**：循环直推 origin/main（用户晨间指令）。
- **新事实**：远端 main 已开启分支保护（require PR，GH013 拒绝直推）。
- **现行流程（PR #13 验证可行）**：每棒完成后 `git push origin HEAD:refs/heads/auto/<功能名>` → `gh pr create --head auto/<功能名> --base main` → `gh pr merge <N> --merge --delete-branch` → 本地 `git checkout main && git pull`。满足保护规则且无需人工；若仓库后续加 required review 导致自合并失败，则退化为「推分支 + 提示人工合并」。
- **注意**：rebase 远端新提交时文档冲突（roadmap/README 修订表）按「两边行都保留」合并。

## #3 AI provider 的 API key

- **岔路**：P1 AI 功能（队列 #3-#5）需要真实模型端点与 key 才能端到端。
- **自动选择**：以 MockProvider（100ms/chunk 流式）交付全部链路与 UI；provider trait 预留 OpenAI/Anthropic/Ollama adapter，key 从 `LATERMD_AI_API_KEY` 或凭据管理读取，**代码不硬编码任何 key**。
- **如何改**：设置界面填 key（或 `export LATERMD_AI_API_KEY=...`），选 provider 后重启即用真实模型。

## #4 依赖新增的登记口径

- **岔路**：循环会给仓库引入 ADR-004 清单外依赖（grep-searcher、dark-light、keyring 等）。
- **自动选择**：每引入一个，同步在 `docs/adr-004-technical-stack.md` 表内登记（版本+用途+notes），视作 ADR 滚动修订。
- **如何改**：若某个依赖不认可，revert 对应 commit，登记行随代码一起回退。

## #5 打包分支 WIP 的贮藏（现状已简化）

- **岔路**：切换 main 时工作区残留未提交改动，阻塞 checkout。
- **自动选择**：stash 无损贮藏，不丢、不代提交。
- **现状**：打包工作已随 PR #7 合入 main；工作区现存 README.md / docs/README.md / docs/roadmap.md 修改与 docs/distribution.md 属 UI 设计线会话恢复的 WIP，归它处置，本循环不再 stash（见 #6）。stash 栈若仍有 `auto-cycle:` 条目，恢复前先 `git stash show -p` 与 main 对比，无增量直接 drop。

## #30 外壳重构 D1–D5 拍板（2026-09-26，依据用户指令自动拍板）

- **岔路**：ui-shell-redesign.md 的五个决策点（D1 自绘无边框标题栏 / D2 菜单栏独立行 / D3 单栏三段 / D4 禅定保留标题栏退出入口 / D5 可视化编辑只预留接口）原计划等坤哥逐条拍板。
- **拍板依据**：用户 2026-09-26 指令「看下现在还有什么未完成的任务，加入流水线」——即放行 #13 进入自动循环；按循环授权（自动选最优解不等人），D1–D5 全部取规格中已论证的**默认选择**。
- **风险兜底**：D1 无边框的三平台拖拽/缩放风险保留 `LATERMD_NATIVE_DECORATIONS=1` 逃生口回落原生装饰（与 `LATERMD_RENDERER=glow` 同构）；M5 收口棒带明暗像素采样验收，Win/mac 真机复测留在人工清单。
- **如何改**：任一决策想推翻，改 ui-shell-redesign.md 对应 §（D1 见 §3、D2 见 §2、D3 见 §5、D4 见 §7、D5 见 §8），在对应里程碑棒完成前修订代价最低；已完成后再改 D1 需同时保留逃生口路径。

## #62 代码块复制按钮(#38)乙案接管通道不可行的探测取证:块体复用三件套均 pub(crate)(2026-09-30 自动选择)

- **岔路**：auto-plan #38 的乙案要求「`is_block_code_widget` 接管全部代码块,块体复用 vendored 高亮/横向滚动」——接管意味着 app 侧自绘块体,复用是否成立决定乙/甲走向。
- **取证(探测棒实读)**：块体复用所需的 `scrolling_code_galley` / `StreamingCodeCache` / `theme_identity_ptr` 在 vendor/egui_markdown/src/layout.rs 尾部(`pub(crate) use syntect_code::{scrolling_code_galley, theme_identity_ptr, StreamingCodeCache}`)均为 **`pub(crate)`,app 侧不可见**;公开的 `highlight_code` 只有裸 LayoutJob,无横向滚动包装与流式缓存。乙案要走通必须先做 vendor ①类改 pub——比直接消费既有挂载点还重。
- **自动选择**：**零 vendor(与 #61① 同结论,独立取证)**——消费上游既有 `MarkdownLabel::code_block_buttons` 挂载点;源文本由 vendored 回调直接透传 `Token::CodeBlock.text`(与 ```ai 指令卡同法)。另记录任务口径出入:auto-plan #38 写「经 code_block_spans 偏移切 rendered」,但 ```ai 先例的实际机制是 vendored 回调透传文本(label.rs `block_code_widget(ui, text, …)`),不经偏移换算;按「以现状为准」取后者。
- **如何改**：若将来确需乙案接管(如块体上方独立行),在 vendor ①类里把上述三件套提为 `pub`(上游可合的通用能力)并登记 vendor/README.md 变更表;落地口径见 #61。

## #66 查找条 Esc 关闭在 egui 0.36 下是哑弹的修复口径(#17 M1,2026-10-01 自动选择)

- **岔路**:#17 M1 规格「Esc 整条关闭沿用现状」,但无头实证发现已合入的查找浮层里 Esc 关闭**本来就是坏的**——egui 0.36 的 `Focus::begin_pass`(egui/src/memory/mod.rs:595-598)把裸 Esc 当「交出焦点」在帧首清焦,而 `find_bar_contents` 的 Esc 检测以 `response.has_focus()` 为前提,永不触发(探针:聚焦查找框发 Esc,outbox 空、`find_open` 保持 true、焦点被 egui 拿走;只有 ✕ 按钮真能关)。M1 红线「不得重做已合入查找浮层、发现缺陷如实交人工不顺手大改」与「Esc 关整条」规格在此冲突:沿用现状=交付一个哑弹键位。
- **备选**:①只修替换行、查找框留缺陷——两框 Esc 行为不一致(替换框行、查找框不行),比统一坏更怪;②两框都修(各加一行 builder 参数);③都不修,Esc 全部交人工。
- **自动选择**:②——给查找/替换两个输入框 TextEdit 都加 `egui::EventFilter{ escape: true, .. }`(TextEdit 出厂默认 arrows 锁框内、escape 不锁),egui 官方为「Esc 应作用于控件而非交焦」提供的出口(builder.rs:351 注释原文举的例子就是 completion popup)。改动是两行 builder 参数,查找行的结构/逻辑零变化,不构成「重做」;修复后无头回归 `find_row_escape_closes_the_bar` / `replace_row_escape_closes_the_bar` 钉住两框行为。
- **理由**:tooltip 与验收口径(acceptance-checklist §9 语境)都承诺 Esc 关闭;规格字面「沿用现状」的意图是保持行为,不是保持缺陷;两框必须同口径,否则用户按 Esc 结果取决于焦点在哪个框。
- **如何改**:若认为这越过了「不顺手大改」边界,revert `ui/layout.rs` 中 `FIND_BAR_EVENT_FILTER` 常量及两处 `.event_filter(...)` 调用即可(查找条回到哑弹 Esc、替换行 Esc 检测保留但同样不触发),两个 Esc 回归测试需同步删。

## #69 主题默认键改排 Alt+T(#45 K1)旧 keymap.json 的迁移口径:值感知迁移「旧默认值→新默认值」,用户自定义/主动清除不动;附带 mac 显示文本 ⌥ 口径(2026-10-01,#45 tab-restore-keymap K1·自动拍板)

- **岔路一(迁移口径)**:任务书要求「旧 keymap.json 已有用户自定义 Theme 绑定的,加载时不覆盖(增量迁移语义,与 #17 同口径)」,但 #17 的字面机制(只给**缺失** command id 补默认、既有条目一律不动)与本任务目标「把 Cmd/Ctrl+Shift+T 空出来给 TabRestore」对存量用户冲突——历史上每次新增命令,`load_from` 的增量写回都会把**当时的全表默认值**落盘,老用户的 keymap.json 里几乎都躺着 `"toggle_theme": "Ctrl+Shift+T"`(当年出厂值,非用户手笔)。若严格不动它:老用户主题继续占 Cmd/Ctrl+Shift+T,K2 的 TabRestore 增量补进去后同键,而消费顺序(`poll_shortcuts` 修饰键位数降序 + `ALL` 序稳定排序)在前的 ToggleTheme 每次抢先吞键——TabRestore 对全部存量用户永久哑键,「空出来」只对全新安装生效。
- **备选**:①严格 #17 字面口径,既有条目一律不动(代价:存量用户 TabRestore 哑键);②**值感知迁移**——仅当 toggle_theme 条目**解析值**== 旧默认(Cmd/Ctrl+Shift+T)时改写为新默认 Alt+T,其他值(用户自定义、主动清除的空串、解析不了的坏行)分毫不动;③启动时整表 reset_all 到新默认(覆盖一切用户自定义,最粗暴)。
- **自动选择**:②——`load_from` 在增量补默认前先做一次「退役默认值」比对迁移(用 `parse_shortcut` 比**解析值**而非原文,`cmd+shift+t` 等别名/大小写手改档同样识别),迁移发生即走既有 `changed` 通路写回 keymap.json(下次启动不重复迁移)。任务书的保护条款「用户自定义 Theme 绑定不覆盖」在 ② 下字面与意图都成立。
- **理由**:默认键变更的产品语义是「没自定义过的用户跟新默认走」,只有显式偏离应被保留;① 会让 K2 的核心验收(Cmd/Ctrl+Shift+T 恢复标签)对存量用户静默失效,比迁移更伤用户;③ 踩「不覆盖用户绑定」红线。已知局限:曾在旧版**显式**把主题改回旧默认的用户与「从未动过」不可区分,会被一并迁到 Alt+T——与「恢复出厂即新默认」的既有语义一致,接受。
- **如何改**:想回 ①,删 `keymap.rs` `load_from` 中 `retired_theme_default` 迁移块及 `theme_default_migration_on_load` 测试(需接受存量用户 TabRestore 哑键或手动改绑);想区分「显式选过旧默认」的用户,需引入「用户改过键」持久标记,超出本轮范围。
- **岔路二(附带口径,mac 显示文本)**:任务书要求「Win=Alt+T / mac=⌥T 三平台口径」。菜单栏走 egui `format_shortcut`,mac 上本就显示 ⌥T(符号集,`can_show_modifier_symbols` 自带字体回退);但设置「快捷键」页/工具条 tooltip 走自家的 `Shortcut::platform_text`,原先 mac 上也写死 "Alt+"。**自动选择**:`platform_text` 在 mac 输出 `⌥+`(保持与既有 `Cmd+` 同款「词+加号」形态,菜单 ⌥T 与设置页 ⌥+T 的差异同今天 ⌘S/Cmd+S 的差异,口径一致),`parse_shortcut` 增加 `⌥` 别名(与 alt/option 同义)保证存档往返与 mac 手改档可解析。副作用:mac 上含 Alt 组合的存档文本随之改变(ToggleRightPreview 由 `Cmd+Alt+R` 变 `Cmd+⌥+R`),旧档 `alt` 别名保留仍可解析,不受影响。**如何改**:不想要该口径,revert `platform_text` 的 mac 分支与 `parse_shortcut` 的 `⌥` 别名即可(mac 显示回 "Alt+")。

## #70 TabRestore(#45 K2)重开失败条目的去留:失败即丢弃并继续下一条,不保留在栈顶(2026-10-01,#45 tab-restore-keymap K2·自动拍板)

- **岔路**:任务书明确把「重开失败(文件被删/移动)时该栈条目是否出栈」列为二选一(①失败即丢弃并继续下一条 / ②失败保留在栈里),要求取舍登记 decisions-pending 并用单测钉住。失败时提示行已按既有口径写「打开失败 <路径>」(`file::read` 的 FileError 文案),分歧只在栈条目去留。
- **备选**:①丢弃并继续——失败条目当场出栈,同一次触发接着尝试下一条,直到某条成功或栈空;②保留——条目留在栈顶,提示行说明后本次触发结束,下一条要再按一次才摸得到。
- **自动选择**:①。`State::restore_tab` 的循环按「pop → open_path → 成功即停 / 失败继续 pop」实现,死条目绝不留在栈里。
- **理由**:本产品心智是「重开=重新读盘」(preview-typography §3.3),文件被删/移走后该条目在**本会话内**永远无法恢复成功;若保留(②),它永远堵在栈顶,此后每次 Cmd/Ctrl+Shift+T 都先撞一次失败才能摸到下一条,任务书验收「连续按 Cmd/Ctrl+Shift+T 能逐条回走完整条栈」被死条目打断;用户已从提示行得知哪个文件打不开,信息不丢。②的辩护场景是「网络盘/外接盘暂时离线、之后又可用」,但该场景下用户重按一次快捷键即可,代价远小于①堵栈的代价;且不做跨会话持久化(已定案),「暂时不可用跨会话找回」本就不成立。
- **如何改**:想改成②(失败保留),把 `state.rs` `restore_tab` 的 `while let Some(path) = self.tabs.recently_closed.pop()` 改为先 `last().cloned()` 试开、失败即 `return`(不出栈),并同步改 `tab_restore_failed_path_noticed_dropped_and_continues` 测试的断言(死条目留栈、alive 不在同一次触发里恢复)。

## #71 文件树带 Git 角标行改「整行宽+整行可点」(#40 filetree-truncate,2026-10-02·自动拍板)

- **岔路**:角标要「贴行尾」且不被省略号挤掉,egui 0.36 的标准做法是 `Button::right_text`(内部 `Atom::grow`)。grow 原子在空间富余时把块撑满可用宽,于是带角标的行从旧 `selectable_label` 的「内容宽」变成「整行宽」,选中/hover 底色与命中区域随行铺满——这是用户可见的行为变化,不是纯内部改写。
- **备选**:①`Button::selectable(selected, name).truncate().right_text(角标)`——角标真·贴行尾(宽栏窄栏都成立),带角标行整行宽/整行可点,无角标行维持内容宽;②手绘整行(仿 `nav_row` 的 `allocate_exact_size` + `Sense::click`)再自绘名字 galley 与角标——行宽口径全部自控,但要复刻 `Button::selectable` 的选中/hover 样式与无障碍信息,表面积大。
- **自动选择**:①。
- **理由**:①复用 `selectable_label` 的同一条渲染路径(后者在 egui 0.36 就是 `Button::selectable(..).ui(..)` 的别名),选中/悬停样式零漂移;「角标贴行尾」由布局原子保证,名字的截断宽度先扣除角标固有宽,省略号永远挤不掉角标;整行可点/整行高亮是树形列表的通行预期(VSCode 同款),把旧行为里「点名字才算点行」的偏差顺带修掉。git 页改动行(`git_status_row` 同走 `badged_row_label`)一并受益。②为了行宽口径自控引入约 60 行手绘样式代码,违背小表面积。
- **如何改**:想回「内容宽」,把 `badged_row_label` 里 `right_text` 的 grow 语义换掉——不用 `Button::right_text`,改在 `ui.horizontal` 里先 `add(角标 Label)` 再倒排名字,或走②手绘;同时改 `tree_row_truncates_long_names_in_narrow_panel` 里「角标贴行尾(距行右沿 < 12px)」的断言。角标与名字的间距现在是 2×`icon_spacing`(出厂 8px,grow 原子两侧各一个 gap),嫌宽可给 `Button::gap(…)` 传更小值。

## #72 #43 M2 混排基线修复只作用于预览族,编辑器/全局 UI 的同根因基线偏差不在本轮修(2026-10-02,#43 preview-font-metrics M2·自动拍板)

- **岔路**:M1 取证确认混排基线偏差的根因是「链头 Inter 与 CJK 回退 face 的行 metrics 表值差」(残差 ≈0.072em,与任何 line_height 设置无关),修法是把链头 override 成 CJK 同款。但 override 挂到哪个族有两条路:①只挂预览专用族(`Inter-Preview` 正文族 + `bold` 别名族链头,本轮实现)——Proportional/SemiBold/Medium 原生族不动,UI 外壳与编辑器的行高维持现状;②把 Proportional 链头直接换成 override 副本——编辑器(TextEdit)与全部 UI 控件的中英混排基线一并对齐,但**所有 UI 文本行高从 Inter 原生 1.21em 涨到 CJK 同款 ≈1.448em(+19.6%)**,菜单/按钮/标签全套界面密度变松。
- **备选**:①预览族隔离(本轮);②Proportional 全局 override(界面密度全局变化);③编辑器单独再开一个 TextEdit 专用族(编辑器基线对齐且 UI 不动,但编辑器与预览行距不同源,且 TextEdit 的字体配置面要单独拉一遍)。
- **自动选择**:①。#43 条目的症状与验收全部落在预览排版;UI 密度是全局产品决策(+19.6% 行高影响每一屏),单方面改掉超出本任务边界,也不是「预览字体修复」的题中之义。
- **理由**:预览与 UI 的排版需求本就不同源(预览是长文阅读排版,CJK 行距下限 1.448em+ 合理;UI 是控件密度,Inter 原生行高是刻意保留);①把修复面精确对准症状面,降级路径(无 CJK 候选/表值解析失败)全部回落现状行为。编辑器混排基线偏差(同根因,TextEdit 走 Proportional)如实保留,现状与修复前一致——不劣化,只是没顺手修。
- **如何改**:若拍板要全局对齐,把 `fonts.rs` `build_definitions` 里 Proportional 链头从 `NAME_REGULAR` 换成 `PREVIEW_REGULAR`(SemiBold/Medium 同理),`preview_body_family` 即可退役回落 Proportional;UI 行高变化的观感需真机人工复核后定。编辑器单独修则按 ① 的同款手法注册 TextEdit 专用族,由 #23(字号行距设置)一并考虑。附带说明:代码块(Monospace 族)的中文注释行高/基线同样未修(代码块 format 的 line_height=None 走链头行高,不经过 vendored floor;链头是 epui 内置等宽字体,与 CJK Mono 的表值差同根因),一并留给 #23 或后续。
- **2026-10-02 坤哥反馈拍板**:编辑器混排基线修理由 #50 M1 落地(`97a701e`)——坤哥 2026-10-02 反馈截图「源码页面字体大小不一,高低不一致」即本条登记的同根因症状在源码页的呈现;采用本条「如何改」预留的「编辑器单独注册 TextEdit 专用族」路径(`fonts.rs` 新增 `editor-mono` 族,经 `TextStyle::Monospace` 档投影统一源码 TextEdit/行号槽/Live 活动块),实测混排基线偏差 +1~+3px→+0.000px(12-24pt 全档×明暗两主题)。实现与任务书有一处方向偏差:反向 override(CJK 等宽 face 副本对齐链头 Hack 表值、链头不动,保「纯 ASCII 行盒不变」否决线),全文见 #75;上文附带说明的代码块面因 vendored 硬编码 `FontId::monospace` 未能一并换族,保留现状,全文见 #76;预览侧与 vendored 均零改动。

## #73 #23 F3 行距滑杆只作用于预览正文,源码编辑器的行距(行盒倍率)不在本轮投影(2026-10-02,#23 font-prefs F3·自动拍板)

- **岔路**:任务书 F3 目标句写「字号/行距真正作用于编辑器与预览正文」,但子任务清单的实现路径是三条互斥分工——①字号→编辑器(`text_styles` Monospace 档投影);②字号→预览(显式 `FontId`);③行距→`markdown_style()` 的 `line_height_ratio` 覆盖(vendored,即预览)。行距在清单里只出现一次且落点在预览:源码模式 TextEdit/行号槽/Live 活动块的行距没有任何投影项,编辑器行盒保持「等宽族链头自然行高 + `spacing.extra_text_line_spacing`(出厂 0)」。auto-plan #23 规格语言同样宽泛(「作用于编辑器与预览正文」),两处文本与子任务清单的实现边界不一致。
- **备选**:①行距只进预览(F3 实施)——编辑器行盒维持现状,源码模式拖「行距」滑杆无感;②编辑器同步投影行距——egui TextEdit 无行距倍率字段,只能把 `style.spacing.extra_text_line_spacing` 设为绝对像素加值:`clamp(0, size × ratio − ctx.fonts().row_height(Monospace 档@size))`,每帧按运行时字体 metrics 换算,且调低(如 1.2)时要 clamp 到 0 兜底自然行高(CJK 行高物理下限,与预览侧 `min_line_height_em` 同款语义)。
- **自动选择**:①。
- **理由**:子任务清单是实现权威(行距若要双投影,会像字号一样写「→编辑器」「→预览」两条);②把行距语义(字号倍率)翻译成绝对像素加值需要依赖运行时 `Fonts::row_height`(字体链与 CJK face 相关,与 `apply` 的 staleness 投影模型耦合更深——size 与 ratio 任一变化都要重算),是一块独立改动,不该以「顺手可做」混进本棒;且 #72 已定源码编辑器的混排基线/行高本就未修(TextEdit 行 metrics 取等宽族链头出厂值),单独拉行距投影会制造「行距对齐了、基线没对齐」的半吊子状态,不如与 #72 的「如何改」里预留的 TextEdit 专用族排版面一起做。写作面(编辑器)行距的体验感知也弱于阅读面(预览)。
- **如何改**:若拍板编辑器也要行距,在 `theme.rs::ThemeSettings::apply` 里追加 `extra_text_line_spacing` 投影(值 = `(size × ratio − row_height).max(0)`,row_height 用 `ctx.fonts(|f| f.row_height(&FontId::monospace(size)))` 现算,与 `apply_font_size` 共用 staleness 槽),并确认 `ui/editor.rs:167` 的行高估算公式(`row_height + extra_text_line_spacing`,既有已含 extra,无需改)、行号槽 `gutter.rs` 与 `live.rs` 活动块的 desired_rows 同步;补「行距调低不裁切 CJK」的边界测试(clamp ≥ 自然行高)。建议与 #72 的编辑器行 metrics 对齐(TextEdit 专用族/override 副本)同棒做,排版面一次对齐。
- **2026-10-02 坤哥反馈拍板**:岔路转正——坤哥 2026-10-02 反馈「源码页面…每行间距也太小感觉」,原自动拍板①(行距只进预览,理由之一是「写作面行距的体验感知弱于阅读面」)被真机体验推翻,按本条「如何改」预留的方案②落地(#50 M2,`0a7f4be`):`theme.rs::apply_font_size` 扩为字号+行距联合投影,`extra_text_line_spacing = max(0, 字号×行距−自然行高)`(自然行高经 `ctx.fonts` 现算投影后 Monospace 档的 FontId,与 F3 共用按值键控 staleness 槽,size/ratio 任一变化当帧重投影),行盒精确=字号×行距(15pt 实测 1.2 档 18.000px/2.0 档 30.000px),clamp≥0 兜底自然行高(「行距调低不裁切 CJK」边界测试已钉);「与 #72 同棒做、排版面一次对齐」的建议同棒兑现——M1 的 editor-mono 族(混排基线 +0.000px)与 M2 行距投影同在 feature/editor-typography 先后落地,不出现「行距对齐了、基线没对齐」的半吊子状态。

## #74 #23 F4 标题呼吸间距的实现落点:任务书指定 `render_token_range` 层 add_space,实际落在 `build_layout` 层透明 spacer 行(2026-10-02,#23 font-prefs F4·自动拍板)

- **岔路**:任务书(#23 F4)与 preview-typography §2.2 均指定「改 `render_token_range`:token 为标题时 `add_space(block_spacing + heading_space_above)`」。但仓库现状里该函数只服务**分段渲染路径**(文档含表格/图片/引用/block widget/scroll 代码块时 `needs_segmentation` 才为 true,`label.rs` 走 `render_segmented`);纯正文+普通代码块文档——LaterMD 预览最常见的形态——走整篇 galley 路径(`render_galley`),**根本不经过 `render_token_range`**,标题间距完全不生效。且即便在分段路径上生效,`add_space(12px)` 是替换空行(标题前换行 token 被 `flush_text_range` trim 掉)而非叠加,12px 反而**小于**正文段落间距(空行行盒 ≈16.9px@13pt×1.3,app 侧 ratio=1.5 时 ≈19.5px),违背计划文档 §2.3 验收③「标题上下呼吸明显大于行间距」。
- **备选**:①`build_layout` 层在 heading 块首 token(前一 token 是 Newline,天然排除 heading 行内 bold/链接切片)前 append 透明 spacer 行(行高 `block_spacing + heading_space_above`),叠在既有空行之上的额外呼吸——两条渲染路径都调 `build_layout`,自动一致,`render_token_range` 的 `before_block`/`after_block` 零改动;②任务书原口径 `render_token_range` 层 add_space,但要覆盖纯文本文档就必须同时把 heading 拉成 segment break——缓存键、块高缓存、视口剔除语义全变,渲染成本与上游可合性双输;③①②并行——分段路径下 spacer 行与 render 层 add_space 双计,直接排除。
- **自动选择**:①。
- **理由**:产品目标(标题上下有明显大于段落间距的呼吸)与两条硬约束(正文观感不变、两条渲染路径行为一致)都指向同一落点。①的数值语义是「叠加」:标题上方总呼吸 ≈ 空行(16.9px)+ spacer(8+4=12px)≈ 28.9px,明显大于段落间距且可通过一个字段全局调;机制先例是同文件 `HorizontalRule` 的透明 spacer 行做法,新增面极小。触发条件保守:文档/flush 段首标题与紧邻块元素的标题(after_block 已 trim 掉空行,heading 是段内首 token)不插 spacer,顶部不撑空、块间距一视同仁;字段归零时 spacer 行高恰等于 `block_spacing`,语义完全回落到现状。无头实证(egui 0.36.2):spacer 行高三档(8/12/48px)精确等于 `block_spacing + heading_space_above`,总高度随标题数×Δ 精确线性,纯正文文档 rows 数与高度对该字段**完全不变**;「纯正文零影响」已钉成测试(否决线)。
- **如何改**:若想要「12px 纯间距」的替代式观感,把 `layout.rs` heading 分支里 spacer 行高从 `block_spacing + heading_space_above` 改为只 `heading_space_above`(触发条件不变);若想要块元素(表格等)后紧跟的标题也吃呼吸,在 `render_token_range` 的 `after_block` 跳完换行后、下一 token 是 heading 时补一段 `add_space(heading_space_above)`(flush 段首无 spacer,不会双计),该情形当前保持普通 `block_spacing`,已有测试钉死该保守语义。出厂 `line_height_ratio=1.30` 本轮未动;真机目视后微调只需改 `heading_space_above` 一个出厂值(app 侧接线见 F5,皮肤 ron 兼容:已存在的预设文件不含新字段走 serde default 4.0 兜底,显式 0.0 是合法偏好会原样保留)。

## #75 #50 M1 编辑器等宽族的 override 方向:任务书指定「链头 override 对齐 CJK 表值」,实际反向「CJK 副本 override 对齐链头」(2026-10-02,#50 editor-typography M1·自动拍板)

- **岔路**:任务书 M1 写「链头=当前编辑器等宽拉丁 face 的副本(行 metrics override/FontTweak 对齐本机 CJK face 表值,同 #43 M2 手法)」——把链头(Hack,行高 1.164em)的表值改写为 CJK 等宽 face 同款(1.448em);但同一任务书的否决线写「纯 ASCII 文档编辑器 rows 数与行盒高度对新族完全不变」。TextEdit 行盒高(desired_rows 的输入)直接由链头行 metrics 决定,链头被 override 则纯 ASCII 行盒必然从 1.164em 涨到 1.448em(15pt 实测 17px→21.7px,+24%),两条指令不可同时满足。
- **备选**:①照任务书字面,链头副本 override 到 CJK 表值——基线归零但纯 ASCII 行盒 +24%,否决线破;②链头不动,在 CJK face 上用 `FontTweak::y_offset_factor` 平移墨迹——layout 基线仍差(y_offset 只移墨迹不移布局基线),且偏差随字号在 +1~+3px 间变化(本机实测 12-24pt),单个 em 比例因子无法在整数字号档全域精确归零;③**反向 override**:CJK 等宽 face **副本**的表值改写为链头 Hack 同款 em 值(#43 M2 同一套 `override_vertical_metrics` 机制、方向相反),链头分毫不动。
- **自动选择**:③。副本只挂 `editor-mono` 专用族(`fonts::FAMILY_EDITOR_MONO`)并替换链尾原生 CJK 等宽条目(链内混入原生 face 会先命中、override 白做,与 #43 M2 预览族同一纪律);字号仍走 #23 F3 的 `TextStyle::Monospace` 档投影,族由 `apply_font_size` 一并投影(`fonts::editor_mono_family`,未注册回落 `Monospace`)。
- **理由**:否决线的本意是把「行距太小」的改动留给 #50 M2(行距滑杆投影 `extra_text_line_spacing`),M1 只对齐基线;③使链头与 fallback 行 metrics 全等,基线公式(`ascent + valign·(行盒−line_height) + 0.5×(链头行高−face行高)`)残差与 #43 M2 同理精确归零,且 advance/outline/win 表不动 → 纯 ASCII 的行盒/折行/行数逐像素不变。无头实证(egui 0.36.2,本机 Noto CJK face 7):混排基线偏差旧族 +1~+3px → 新族在 12-24pt 全档 × 明暗两主题下 +0.000px,纯 ASCII 文档 rows/逐行行盒/总高与旧族全等(`editor_typography_acceptance` 三测试);`#43 M2 手法`的机制一致性与否决线在此不冲突——冲突的只是改写方向。CJK 与拉丁的墨迹高比(1.25@15pt,face 设计值)未动:任务书第 3 条的「视觉尺寸均衡」经基线对齐后已达「同行无高低差」,进一步缩放 CJK 观感(如 `FontTweak::scale`)需连动 override 目标值(否则破坏基线恒等),留真机目视后再定。
- **如何改**:若拍板要 #43 M2 原方向(链头对齐 CJK、行高随 CJK 涨),把 `fonts.rs` `build_definitions` 编辑器族分支的目标/源头对调(patch "Hack" 字节到 CJK mono face 表值作链头副本),并重锚 `install_registers_editor_mono_family_with_aligned_baseline` 与 `editor_typography_acceptance` 的行盒不变断言(纯 ASCII 行盒将 +24%,需与 #50 M2 的 `extra_text_line_spacing` 语义对表,避免行距双重放大)。

## #76 #50 M1 的「源码高亮代码块」等宽面未能指向新族:vendored 硬编码 `FontId::monospace` 且预览代码块共用同一族,app 侧单独换族不可达(2026-10-02,#50 editor-typography M1·自动拍板)

- **岔路**:任务书第 2 条要求编辑器全部等宽渲染面(源码 TextEdit、行号槽、Live 活动块、**源码高亮代码块**)统一指向新族。前三者都经 `TextStyle::Monospace` 档,族投影一处生效;第四个(带 syntect 高亮的代码块 widget)由 vendored 层渲染,`vendor/egui_markdown/src/layout.rs`(626/643/732/969 等处)硬编码 `FontId::monospace(code_font_size)` = `FontFamily::Monospace` 族,**且预览面板的代码块走同一硬编码路径**——app 侧没有任何可单独给「编辑器里的代码块」换族的扩展点。任务书同时规定本棒「全程不触碰 vendor/egui_markdown/」与否决线「预览侧零改动」。
- **备选**:①给 vendored `MarkdownStyle` 加 code 族字段或在 layout 读 context 标记(①类 vendored 补丁)——触碰 vendor 红线;②直接把 `FontFamily::Monospace` 的链头换成 override 副本——预览代码块纯 ASCII 行盒同步 +24%、含 CJK 注释的代码渲染变化,否决线二破;③不换,现状保留。
- **自动选择**:③。
- **理由**:①违反本棒「不触碰 vendor」的显式约束;②把编辑器修复外溢到预览(预览行距/行盒是 #23/#50 M2 的领地,代码块拉丁行从 17px 涨 21.7px 是可见回归);③的代价可控——代码块拉丁字形与新族链头同为内置 Hack face,纯拉丁代码渲染逐像素一致,唯一保留的现状缺陷是「代码块内 CJK 注释与拉丁的基线差」(同根因,#72 已登记),不影响坤哥反馈的源码 TextEdit 主症状。
- **如何改**:若拍板代码块也要基线对齐,走①类 vendored 最小补丁(`MarkdownStyle` 增 `code_font_family: Option<String>` 之类字段,layout 的 `FontId::monospace` 处改读字段、None 回落现行为,独立 `vendor:` commit + vendor/README 登记),app 侧把编辑器语境的 `MarkdownStyle` 填 `editor-mono`、预览填 `Inter-Preview` 或 None;或拍板全局换 `FontFamily::Monospace` 链头(一行改动,预览代码块与全部 UI 等宽文本随之变化,需真机目视行盒 +24% 的观感)。

## #77 流式 O(n) 的主导修复(布局缓存失效粒度块级化)确需 vendor ①类改动,#46 R2 红线内不可修,移交人工拍板(2026-10-02,#46 streaming-perf R2·待人工拍板)

- **岔路**:R1 复测判定流式追加仍 O(n)(10000 行档 1001.7 ms,较 M0 +26.9%,[perf-recheck-2026-10.md](perf-recheck-2026-10.md) §2/§5),R2 修复路径激活;但 R1 交接的修复对象(疑点 A:布局缓存失效粒度从「整篇 text_hash」降到「块/段」)机制位置**全部在 vendored 层**——顶层 `CachedMarkdownLayout` 以整篇文本哈希为门控(`vendor/egui_markdown/src/label.rs:57-65` `hash_text`、`:636` 命中判定),miss 即 `parser::parse` 全文解析 + `tokens_to_owned` 全量深拷贝(`label.rs:663-665`),纯代码块等 `needs_segmentation=false` 文档更是整篇 `build_layout` 单 galley(`label.rs:669-691`、`layout.rs:262-275`),块级剔除缓存的 key 同样是整篇哈希(`label.rs:137-151` `try_cull_block`、`:772/:808/:837` 调用点)——改任何一处都动 `vendor/egui_markdown/src/label.rs`。#46 R2 模块约束「不触碰 vendor/,修复确需 vendor 时把需求记 decisions-pending 交人工拍板,本模块按 app 侧可达近似或如实报未修」。
- **备选**:①**vendor ①类补丁(两步)**:第一步放宽 `needs_segmentation` 准入(按 token 数/字节数或代码块行数阈值,阈值挂 `MarkdownStyle` 可配、默认保守)让长文档进分段路径;第二步把块级缓存 key(`try_cull_block`/`cache_block_height` 的 `text_hash`)换成「块内容 hash」(token 在手逐块可算,追加尾行只失效尾块);flush 段缓存已是 per-range(`label.rs:975-976` `hash_flush_context` 按 token 切片)可直接受益;整篇 galley 路径的全文 parse+深拷贝占比 ~0.13%(R1 实测外推)可接受不动。②**app 侧近似**:逐一核对后**不存在**——heal 已条件化(#39,`ui/layout.rs:301-310` 三栏与 `:671-674` Live 两处仅 AI 流式写入本标签帧开)、`LinkHandler::id()` 用默认 0 稳定(`app ui/preview.rs` 未 override,`vendor link.rs:92-94`)、widget id 只含 tab id(`ui/preview.rs:539-541`)、修订号纪律在位(`ui/editor.rs:295-299` 仅 rev 前进重建快照);开 `scroll_code_blocks(true)` 强制分段**不构成修复**——块级 cull key 仍是整篇 text_hash(追加仍全块失效),且代码块渲染形态改滚动窗格是观感回归。③**产品侧降级(app 可达,属行为变化)**:AI 流式写入时预览降级纯文本排版,或对流式文档设规模上限,`m0-report` §4.2 曾列的备选方向。
- **自动选择**:本模块不动任何代码(红线);修复需求按①+③登记移交人工拍板;#46 R3 复测按「未修」口径落档(水位即 R1 水位),M0 验证 4 挂账不销。
- **理由**:主导成本(10000 行档 ~99%,全文解析仅 ~0.13%)在 vendored 整篇 galley `build_layout` 与整篇哈希门控;app 侧四候选方向(流式帧免全文重解析/heal 路径/布局缓存命中/无关帧重建)逐项实读核对均「已在位」或「不可达」,任何 app 侧修补都不改变 O(n) 判定曲线;擅动 `scroll_code_blocks` 有观感回归且被 cull key 机制证明无效,属投机优化不做。
- **如何改**:拍板①时——按「先分段准入、再块级 key」两步走 vendor ①类最小补丁(独立 `vendor:` commit + vendor/README.md 变更表 ①类登记 + `vendor/egui_markdown/check.sh` 六项;①类按上游 CONTRIBUTING 标准保持可 cherry-pick),注意两处连带语义:`debug_assert!(layout.segment_breaks.is_empty())`(`label.rs:692`)的两侧一致性约束、以及块序号作 id 成分时编辑中部不得挪移后续块序号(AGENTS §6.7 widget id 纪律);完成后按 perf-recheck §2 同一条命令复测,验收口径 = 每行成本随规模趋稳(亚线性),M0 挂账凭新数字销账。拍板③时——改动局限 crates/(流式路径按文档规模切渲染配置或关流式预览),一个 PR 可完成,但流式写作的产品体验降级需坤哥先认。

## #78 #42 M1 偏移→预览块位置通道选 vendor ①类而非 app-only(2026-10-03,#42 outline-preview-jump M1·自动拍板)

- **岔路**:任务书给「vendor ①类(渲染时记录块 span→rect 表 + pub 查询)」与「app-only 替代方案」两条路,要求自选并记录理由。探测结论:vendored `render_token_range` 的块 widget 分支(表格/代码块/图片/引用/AI 指令卡)全部在 vendor 内部渲染,app 侧无任何挂载点能拿到它们的 rect(`code_block_buttons` 回调只有按钮行的 Ui,表格完全无回调);`section_anchors`(2026-09-26 已有)只在单 galley 路径有效,分段文档每 flush 覆盖写同一 data key 且 byte_start 是段内偏移——含表格/代码块的文档查表必错。app-only 不可达,通道必须开在 vendor。
- **备选**:①vendor ①类块表(`BlockSpanRect` + `block_span_rects`/`block_rect_at_offset`,记录点覆盖 render_galley 细块/块 widget 粗块/flush culling 粗块);②app-only 探针(照 `copy_button_rects` 手法在可挂载的回调里记 rect)——只能覆盖代码块按钮行,表格/图片/引用全盲,否决;③修旧 `section_anchors`(分段聚合 + 块 widget 补锚)——行级锚点模型(单 y、无 span)承载不了「块内偏移归属/间隙归后续块」的查询语义,改造量等于重写。
- **自动选择**:①。
- **理由**:块 widget 几何只有渲染它的那一层知道,①是唯一完整覆盖;帧号键控的表(首写重置 + 跨帧读 None)天然满足「查询结果随缓存失效」——屏幕坐标过期即不可用,文档变更随重渲染同帧重建;①类按上游 CONTRIBUTING 标准(纯新增 API + 独立测试 + 不动既有渲染路径),`vendor/egui_markdown/check.sh` 六项全绿,可 cherry-pick。实现中推翻了一个直觉方案:`section_to_token` 不能反查行 y——epaint `LayoutJob::append` 会合并同格式相邻 section(text_layout_types.rs:205-213),两序列索引不平行;改用块首文本在 job.text 单调正向查找 + `Galley::pos_from_cursor(prefer_next_row)` 定位(O(doc) 单遍,测试钉死单调性)。
- **如何改**:想换回行级锚点模型(③),删 label.rs 的 `BlockSpanRect`/`block_span_rects`/`block_rect_at_offset`/`is_block_start`/`block_needle`/`record_text_blocks` 与三处记录点,把 `section_anchors` 改成跨 flush 聚合(offset 换算 + 块 widget 手工补锚),app 侧查询退回「锚点 y 二分」;测试 tests/block_span_rects.rs 同步删。

## #79 #42 M1 大纲跳预览的滚动语义取「一次到位 + 目标居中」(Align::Center + ScrollAnimation::none)(2026-10-03,#42 outline-preview-jump M1·自动拍板)

- **岔路**:任务书跳转语义原文「预览 scroll_to_rect 一次到位居中」——「居中」可读作目标块滚到视口中央(Align::Center),也可读作「跳转到位」的笼统说法、实际按编辑器侧惯例对齐视口顶(Align::TOP,2026-09-26 旧实现即 TOP);「一次到位」可读作禁用滚动动画(ScrollAnimation::none)或仅「不要分步多次滚动」(默认动画也算一步请求)。
- **备选**:①`scroll_to_rect_animation(rect, Align::Center, ScrollAnimation::none())`——点击即达、目标块居中;②`Align::TOP` + 默认动画——沿用旧行为与 egui 默认平滑滚动;③Center + 默认动画——居中但走动画。
- **自动选择**:①。
- **理由**:按任务书字面直译;「一次到位」排除动画(平滑滚动在长文档里目标是屏外千行级距离,动画观感是「飞过半个文档」,编辑器内跳转同类场景也是瞬达);跳转目标是大纲点击的标题,居中让标题上下文(前后文)同时可见,长文档跳到底部标题时 TOP 会让目标贴顶失去「到了哪」的参照。端到端测试 `outline_jump_actually_scrolls_preview` 断言滚动真实发生(这是用户反馈「目前就源码跳转了」的回归锁:旧实现的 scroll_to_rect 写在 ScrollArea 闭包外,pass_state 滚动目标在下一帧 begin_pass 被清空,永不消费——egui 0.36 `PassState::begin_pass` 实读 + 双向对照实验实锤)。
- **如何改**:想改回顶部对齐或平滑滚动,只动 `crates/latermd-app/src/ui/preview.rs` 消费段的 `Align::Center`/`ScrollAnimation::none()` 两个实参;真机目视若觉居中跳动大,改 `Align::TOP` 一处即可。

## #80 #42 M2 预览面板收起期间的大纲跳转请求取「悬置,重开补跳」而非「即弃」(2026-10-03,#42 outline-preview-jump M2·自动拍板)

- **岔路**:右栏收起时点大纲(左栏大纲仍可见,这是真实路径),预览侧请求(`PreviewState::scroll_target`)怎么处置:①悬置——重开面板的第一帧消费并补跳到目标;②即弃——面板不可见就当请求作废,重开停在原地。任务书只写了「预览面板不可见时消费请求无 panic」,没有给可见性语义,两条都不与之冲突。
- **备选**:①悬置补跳(零代码 delta:消费只发生在 `preview::ui` 渲染帧,收起时本就不跑);②归约侧判 `layout.right`,不可见直接把 `scroll_target` 清掉(要给归约加 UI 可见性判断,与「滚动位置不是文档状态、不进归约」的既有口径相悖);③收起时挂起、重开时若期间有编辑才丢弃(现行为已是这个并集:悬置 + rebuild 即弃,见 `outline_click_residual_request_dropped_after_rebuild` 与 layout 测试第二段)。
- **自动选择**:①(即现状 ③的并集)。
- **理由**:用户点大纲的意图是「带我去那」,面板当时看不见不等于意图消失,重开补跳最贴近意图;文档变更后旧偏移对新文本无意义,rebuild 丢弃兜住「补跳跳错文本」的风险;①不新增任何生产代码,也不把可见性判断漏进归约。落档测试 `outline_click_with_preview_collapsed_pending_then_dropped_after_edit`(完整 draw 路径:悬置→重开补跳→收起→变更→丢弃→重开不复燃)。
- **如何改**:想改成「即弃」,在 `Message::RightPanelToggled(false)` 的归约里加一行清 `scroll_target`(归约本来就知道面板开关),并把 layout 测试第一段「重开消费」的断言翻转为「重开仍在原地」;悬置语义的文档口径在 `state.rs` 的 `scroll_target` 字段注释,同步改。

## #81 LP2-1 半隐藏的显形口径与弱化档位:caret 紧邻即显形 + text_edit 背景色 α0.65 常量,块级标记不弱化(2026-10-03,#14 lp-v2 LP2-1·自动拍板)

- **岔路**:①规格只说「光标(焦点)落在某段标记上时,该段标记显形」,「落在」的精确边界未定义——严格内部(`cs<c<ce`,两字符标记如 `**` 仅剩一个可触发位置)还是紧邻即显形(`cs<=c<=ce`);②弱化的实现档位——遮罩色与不透明度取什么、要不要做成用户设置;③「标记」的外延——只内联(`**`/`*`/`_`/`` ` ``/`~~`/链接括号/图片/autolink)还是连块级(`#`、列表符、表格竖线、任务框 `[ ]`)一并弱化。
- **备选**:①显形:严格内部 / 紧邻即显形 / 整对联动(光标在 `**bold**` 内容里也显形两端,Typora 风格);②档位:字形重排着色(自定义 layouter,与本帧光标有一帧错位)/ 背景色遮罩 α 可调 / 背景色遮罩 α 常量;③外延:仅内联 / 内联+块级。
- **自动选择**:①紧邻即显形;②`text_edit_bg_color` × α=166(≈0.65)编译期常量(`MARK_FADE_ALPHA`,live.rs),不做用户可调配置面(规格明文);③仅内联。
- **理由**:①egui 点击定位把光标放在被点字形的边界上,严格内部会让「点一下标记显形」成为哑路径(测试 `live_click_on_faded_mark_focuses_editor_and_reveals` 钉住点击显形);整对联动与「该段标记显形」的字面不符且常亮削弱「接近渲染效果」的目标;②遮罩与编辑框底色同源才能读作「文字变淡」而非「贴了高亮」;α0.65 按混合公式字形残留约 35% 亮度(数值推断,真机目视留人工,见 notes),用户可调会新增设置面,规格明文不做;③任务书名即「内联标记半隐藏」,块级标记弱化会碰 `#`/列表符的编辑可读性,且 `inline_marks` 的 gap 机制扩展到 Heading 是一行 match 的事,留作后续增量。
- **如何改**:①想改严格内部,把 `paint_mark_fades` 里 `start <= caret && caret <= chars` 收紧为 `start < caret && caret < chars`(live.rs,测试 `live_reveals_mark_under_caret_or_selection` 第二段同步把光标 1 断言改到仅内部位置);②想调 α,改 `MARK_FADE_ALPHA` 一处常量;想做整对联动,需要在 `inline_marks` 输出上补「配对标记同组」元数据(区间成对标记),显形判定按组;③想把块级标记纳入,在 `latermd_md::inline_marks` 的容器 match 里加 `Tag::Heading`(Start/End 区间同为整构造,gap 机制现成),并补 heading 样式的 md 单测。

## #82 LP2-2 选区扩展的交互边界:双击吞标记、拖选锚点口径、嵌套取最内层、键盘/三击不扩展(2026-10-03,#14 lp-v2 LP2-2·自动拍板)

- **岔路**:①双击选词是否吞掉标记(词扩到 `**词**` 还是保持裸词);②拖选半程松手的落点——何时把选区吸附到完整标记对;③嵌套构造(`[**b**](u)`、`***x***`)扩到哪一层;④拖选「从标记内侧发起」的锚点边界口径(单字符标记 `[`/`` ` `` 没有内部位置);⑤键盘选区(Shift+方向键)与三击选行是否同样扩展。
- **备选**:①吞标记 / 不吞(裸词,传统编辑器行为);②吸附时机 = 拖选全程持续吸附 / 仅松手帧吸附 / 从不吸附 / 「选区两端只要都在对内」就吸附;③最内层 / 最外层;④锚点严格在标记字符之间(单字符标记不可及)/ 压着标记含两侧边界;⑤键盘与三击同样扩展 / 不扩展。
- **自动选择**:①吞——双击词扩到**包含该词的最内层**标记对(内容+两侧标记);②仅松手帧、且要「锚点压着某一侧标记发起 + 本次选区整个含在该构造内」两个条件同时成立才一次性吸附(锚点=按下帧的塌缩光标,press 帧记、release 帧消费);③最内层(构造区间最短的一对);④闭区间 `opening.start <= anchor <= opening.end`(闭侧同理)——压上即算;⑤不扩展,只有指针双击/拖选两种手势触发。
- **理由**:①任务书明文目标是「让编辑标记本身有可及入口」,双击是最高频入口,吞掉标记后选区盖住整对、两段标记随选区显形,打字即连标记一起替换;②持续吸附会在拖选过程中反复改写选区(与用户手感对抗),「选区两端都在对内就吸附」会把长粗体段里的普通拖选吹胀成整段(最小惊讶的对立面);锚点条件保证只有「从标记上发起」的拖选才扩,从内容中部发起的普通拖选原样保留(测试 `live_drag_release_expands_only_from_mark_anchor` 双向钉住);③最内层=词直接所在的那一对,贴直觉;外层可通过先选中内层后再选外层文本间接达成;④egui 光标只能落在字符边界上,`[`/`` ` `` 这类单字符标记只有两个边界位置,严格内部口径会让它们永远不可及;⑤键盘选区是精确操作,被静默改写最令人惊讶;三击选行与跨对选区同理,且选行/跨对时「选区整个含在构造内」多数自然不成立。扩展是一次性的(仅事件帧),静止帧不吸附。
- **如何改**:①改双击不吞,把 live.rs 手势判定里 `double_clicked` 分支删掉(只留拖选),测试 `live_double_click_inside_pair_expands_selection_once` 翻转为断言选区停在词上;②放宽/收紧吸附条件在 `latermd_md::mark_interaction` 的 `anchored` 谓词与 `within(construct, selection)` 过滤(想全程吸附则把 `MarkGesture` 从事件帧改为每帧传入并跳过 `expanded != 当前选区` 的守卫);③改最外层,把 best-pair 的比较从「构造最短」翻成「构造最长」(latermd-md `mark_interaction`);④改严格内部,把两处闭区间 `<=` 收成 `<`(注意单字符标记将不可及);⑤想把键盘选区也纳入,需要在 live.rs 键盘事件帧合成 `MarkGesture` 传入(现状无此路径,键盘选区不经手势判定)。

## #83 LP2-3 点击进编辑的光标写回不触发视口跟随(仅键盘导航/跨块路由/大纲跳转触发)(2026-10-03,#14 lp-v2 LP2-3·自动拍板)

- **岔路**:pending_caret 落地帧的视口跟随是否区分来源 —— 点击富渲染块进编辑(v1:光标落到块末)也是一次程序化光标写回,跟不跟随。
- **备选**:①所有 pending_caret 落地帧一律跟随(实现最省,来源无关);②仅跨块路由/大纲跳转跟随,点击进编辑不跟随;③点击进编辑改为光标落在点击行并跟随(要新增点击点→块内偏移换算,v1 落块末的语义一并改)。
- **自动选择**:②。`LiveState::caret_follow` 与 pending_caret 同生共死:路由/jump_to 置真,activate 置假。
- **理由**:点击是指针交互,用户刚用指针把视口定位到点击处,同帧再把视口拽到光标落点(块末)等于抢滚动(#29「滚轮/空闲帧不抢滚动」的同族红线);对高度超视口的长块尤其明显 —— 点块首、视图跳块尾。路由/跳转是键盘/结构性移动,光标去了别的块,视图不同帧跟上就会出现「光标写了但视图没跟」。③超出 LP2-3 范围(v1 点击落块末是既有语义)。
- **如何改**:想改①,把 live.rs `activate` 分支的 `live.caret_follow = false` 删掉(该字段默认即真);想改③,需要在富渲染分支记录点击命中的块内行,`pending_caret` 给行首偏移,并同步改 `live_click_outside_copy_button_still_enters_edit` 的断言。

## #84 LP2-3 Live 大纲跳转的目标块按「字节归属」而非「标题内容块」(2026-10-03,#14 lp-v2 LP2-3·自动拍板)

- **岔路**:jump_to 的字符偏移落到 v1 块表时,目标块怎么取 —— 块表现状里标题的 `# ` 标记可能归上一块尾(实测 `# 顶\n\n…\n\n# 底\n` 的底标题 `# ` 字节属段落块),按字节归属会激活段落块而非标题内容块。
- **备选**:①`block_containing(跳转字节)` —— 字节属哪块就激活哪块(与 sync/路由的既有不变量一致);②跳过标题标记字节(`#`/空格)落到标题内容所在块;③改 latermd_md::blocks 让标题标记归标题块(块表语义变更,波及 v1 全部行为)。
- **自动选择**:①。
- **理由**:光标字节与源码模式 jump_to 的落点完全相同(标题行首),「同一入口同一时序口径」在光标位置上严格成立;视口滚到光标行,标题行必在视口内,跳转观感达成。②引入 app 侧对标题语法的二次猜测,与「结构化操作在 token/AST 层」铁律相悖;③是块表 v2 的课题,不在 LP2-3 内夹带。
- **如何改**:想改②,在 live.rs jump_to 消费处对字节做「跳过 #/空格」换算(注意 Setext/列表等形态会猜错,不建议);想改③,动 `latermd_md::blocks` 的 heading 段起点(连 `blocks_cover_the_whole_document` 等既有断言一起改)。

## #85 LP2-3 接受 egui 0.36 焦点锁单帧窗:授予帧的下一帧按裸方向键会被记忆层焦点导航抢焦,app 侧不绕(2026-10-03,#14 lp-v2 LP2-3·自动拍板)

- **岔路**:egui 0.36 的 `set_focus_lock_filter`(TextEdit 捕获方向键、阻止记忆层焦点导航)要等「持焦的下一帧 show()」才生效;`request_focus` 授予帧的紧邻下一帧若出现裸 ↑/↓,`Focus::begin_pass` 会按 cardinal 导航把焦点交给相邻可聚焦 widget(Live 面板里富渲染块 `Sense::click_and_drag` 含 FOCUSABLE 位,候选存在)。窗口宽一帧(≈16ms)。app 侧是否人为闭合它。
- **备选**:①接受窗口(人类点击→按键间隔 ≥100ms,永远跨过它;测试按真实节奏在授予帧后空转一帧);②授予前一帧提前 `request_focus`(窗口宽度不变,只是挪位置);③每次落地帧后主动监测「焦点被抢」再抢回来(启发式,与框架对抗)。
- **自动选择**:①。
- **理由**:②实测无收益(窗口仍是一帧,且提前给尚未渲染的块授焦引入新边界);③是在模拟框架行为,必然跟 egui 内部实现耦合,升级即碎。源码栏不受影响的唯一原因是其面板内恰无其它可聚焦 widget(碰巧而非设计),Live 面板富渲染块天然可聚焦,窗口才显形 —— 这是 egui 0.36 框架级时序,不是本仓库代码缺陷。
- **如何改**:若上游修复或升级 egui 后窗口消失,删掉 live.rs 测试 `live_keyboard_navigation_scrolls_caret_into_view` 里授予帧后的空转帧注释段即可收紧测试;真要 app 侧闭合,唯一干净路径是在 vendored 层给活动块 TextEdit 换 `.event_filter(EventFilter{ vertical_arrows: true, horizontal_arrows: true, tab: false, escape: false })` 之外再想办法提前授焦 —— 属 vendor 变更,须按 §6 登记。

## #86 LP2-4 统一撤销评估:维持「按块分段的 TextEdit 内建 undoer」,不换自维护 undoer(2026-10-03,#14 lp-v2 LP2-4·评估+自动拍板)

- **岔路**:#25 已知边界「Live Preview 每块 undo 快照是本块的,跨块撤销按块分段」,LP2-4 要求评估「换成自维护 undoer 换全文统一撤销」的代价后定去留。机制现状(本会话实读):undo 走 egui 0.36.2 TextEdit 内建 undoer,快照 = `(CCursorRange, String)`(整篇文本 + 光标区间,egui `util/undoer.rs:12`、`text_edit/builder.rs:1085-1087` 帧前后各 feed 一次),undoer 挂在按 **widget id** 键控的 `TextEditState`(每块独立一栈,`max_undos: 100`);源码模式整篇一个 id(`tab_editor_id`),Live 模式每块一个 id(live.rs `editor_id.with(("live-block", index))`)—— 块 id 不同 → 栈不同 → Ctrl+Z 只撤当前块内的修改。
- **备选**:①换自维护 undoer(latermd-editor 建全文快照栈,live.rs 活动块改只读渲染 + 自管编辑路径,统一 Ctrl+Z 跨块一步回);②维持分段(现状);③中间路线「跨块撤销桥」:保留内建 undoer,当前块栈撤空时把焦点与下一次 Ctrl+Z 路由到上一个编辑过的块(经 pending_caret 路由,不动 undoer 本体)。
- **自动选择(评估结论)**:②维持分段;③登记为将来有真实用户反馈时的首选升级路径。产品行为同步写进用户可见文档(README.md「功能特性 · 编辑与预览」:Live 模式下撤销按块分段)。
- **理由**:①的代价逐项过 —— ⑴**IME 组合要自兜**:egui 的快照分组是 flux/stable_time(1s)/auto_save(30s)时序语义(`undoer.rs:178-225`),组合(preedit)期间文本不进 buffer、commit 时一次性落一个点;自维护若把 preedit 变更当编辑推栈,撤销会在组合中间撕开文本。AGENTS §8「IME 是头号风险」,#19 刚把候选框跟随做稳,为撤销语义重开 IME 边界不值。⑵**选区/光标行为要自兜**:内建快照带 CCursorRange,撤销恢复文本的同帧恢复光标;自维护要每步快照自带光标,跨块撤销还得走 live.rs 的 pending_caret 跨块路由 + caret_follow 时序(LP2-3 刚收口的口径)。⑶**内建交互竞争要自兜**:Ctrl+Z/Ctrl+Shift+Z/Ctrl+Y 在 TextEdit 事件循环内部消费(`builder.rs:1195-1230`),换自维护要么 event_filter 拦键要么关内建,双栈并存必错;拦键后还要处理「焦点不在编辑器时 Ctrl+Z 归谁」。⑷**快照成本**:内建按整篇 String 存(每槽一份,上限 100 份);全文统一撤销沿用快照式则每推一点拷整篇(长文档下百份快照的内存/拷贝成本),delta 化又是一块独立工程。⑸**收益面**:块内撤销是绝对主频;分段撤销是「语义分段」的体验缺口而非功能缺失(每次 Ctrl+Z 撤当前块内一步,仍可用),当前无用户反馈驱动。
- **如何改**:要统一撤销走① —— latermd-editor 加自维护快照栈(文本+光标,flux 分组),live.rs 活动块 TextEdit 换非交互渲染或锁内建 undoer,预估一个独立 feature 棒(IME/选区/拦键三面都要无头+真机验收);要较轻的跨块连续撤销走③ —— live.rs 维护「最近编辑块序号栈」,块 undoer 撤空(该次 Ctrl+Z 无变更)时转移焦点到上一个编辑块,不动 undoer 本体、IME 零风险;推翻本条(换回①)时同步回改 README 的行为说明与 #25 的「已知边界」表述(该边界随本条从「接受」升格为「有意为之的产品行为」)。

## #87 BK1 反向链接扫描(#15)的四处口径:路径式目标按相对路径匹配、同步直扫无后台服务、自链计入、悬空引用计入(2026-10-03,#15 backlinks BK1·自动拍板)

- **岔路**:任务书把四处自由度交给实现者:①路径式目标 `[[dir/doc]]` 按文件名末段还是相对路径匹配;②API 形态取 SearchService 同款后台服务还是同步函数(任务书明示「反向链接量级小,允许同步直扫」);③自链(`note.md` 里写 `[[note]]`)计不计入结果;④悬空引用(目标文件不存在)计不计入结果。
- **备选**:①末段匹配(任何目录的同名文档都收,Obsidian 部分插件行为);②后台服务(代际号 + channel 流式回传);③自链排除(「谁链接了我」直觉上不含自己);④悬空不计数(扫描时校验目标在盘上存在)。
- **自动选择**:①**相对路径匹配**——目标含 `/`(或 `\`)时,目标串与文档相对路径两侧都归一化分隔符、末段剥一个 `.md`/`.markdown` 后缀、忽略大小写后全等;目标不含 `/` 时按文件名 stem 忽略大小写全等(`[[同名]]` 与 `[[同名.md]]` 互容,扩展名本身不限 md/markdown)。②**同步直扫**——`latermd_search::backlinks(root, doc, stop, max_hits)`,`stop` 短路保留(大仓可中断,中断返回已收到的部分且不置 truncated);收全 → 排序(来源路径, 行号, 目标)→ 截断,与 `list_files` 同款纪律保证截断确定性。③自链**计入**。④悬空引用**计入**(纯文本匹配,不查盘上存在性)。
- **理由**:①与预览侧 `find_by_name`(decisions-pending #26)的正向解析对称——带 `/` 的目标正向只按相对路径直取,反向若按末段匹配会出现「面板说有反向链接、点过去却打开另一目录的同名文档」的错位;不同目录的同名文档(`dir/goal.md` 与 `other/goal.md`)不被 `[[other/goal]]` 误伤;大小写不敏感与 stem 口径(#26)在扫描内部保持一致,不因目标是否带 `/` 而忽明忽暗。②反向链接量级小(单文档被引常态几十以内),面板要的是完整列表而非流式增量;`SearchService` 的 channel + 代际号是为「边出结果边取消」的全库正则搜索设计的,为一次直扫复制一套线程机制只增表面积;要后台化的消费方拿 `stop` 闭包自行包线程即可。③自链是真实引用(文档内目录、回头引用常见),扫描层忠实呈现,要不要在 UI 层折叠展示留给面板模块决定。④不查存在性使扫描无「先列文件再匹配」的顺序依赖,且天然覆盖「未落盘/未保存文档已被引用」的场合;被查询文档存在时,按上述口径命中它的链接按定义不悬空,两条口径不打架。**已知边界**:无 `/` 目标按文件名全局匹配,同名文档多篇时(如 `a/goal.md` 与 `b/goal.md` 各被 `[[goal]]` 引用)两篇各自查询都会收到该引用——与 `find_by_name` 正向「取列表首个命中」的模糊性同源(#26 既有),不在此放大;目标原文中的 `#` 锚点(`[[doc#章节]]`)不剥,正向同样不解析,维持对称;反向的 `.md` 后缀容错比正向宽(正向 `find_by_name` 对无 `/` 目标不剥后缀),`[[note.md]]` 会计入反向链接但点击未必正向打开——面板展示目标原文可让用户识别写法。
- **如何改**:①要末段匹配——把 `target_matches_doc`(crates/latermd-search/src/lib.rs)的含 `/` 分支改成取末段走 stem 比较;要路径匹配大小写敏感(与 Linux 正向直取完全对称)——`path_key` 去掉两处 `to_lowercase`;②要后台服务——照 `SearchService` 的代际号模式包一层线程,`backlinks` 本体不动;③要排除自链——面板层按「来源相对路径 == 目标文档相对路径」过滤,或给 `backlinks` 加 `include_self: bool` 参数;④要悬空不计数——匹配前先 `find_by_name` 验证目标可解析(代价:每条候选多一次全库列举,需缓存);要让 `[[note.md]]` 正向也能打开——给 `find_by_name` 的无 `/` 分支补同款 `strip_md_extension` 容错(app 侧 filetree.rs,与本条正向对称收口)。
