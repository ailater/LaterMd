# P0 验收状态核对表(2026-10)

日期: 2026-09-30
产出: #44 核心验收与文档状态纠偏 —— V1 事实核对(只读取证与核对表);V3 自动验收与 blocked_external 登记见 §8
关联: [p0-acceptance.md](p0-acceptance.md)、[acceptance-checklist.md](acceptance-checklist.md)、[roadmap.md](roadmap.md)「当前位置」、[distribution.md](distribution.md) §2–§4、[README.md](../README.md) 安装节、[decisions-pending.md](decisions-pending.md) #53(第三轮评审补全)

> 本表由 V1 只读取证棒产出:**全部命令只读**(gh 仅 list/view/api GET;git 仅只读子命令)。取证命令与关键输出见文末「证据摘录」,行内证据编号 E1–E22 一一对应。
> §8 由 V3 棒追加(自动验收实跑证据,编号 V3-E1…,与 E1–E22 两套编号不混用)。
> **事实基线(2026-09-30 取证)**:最新 Release = **v0.0.3**(2026-09-30T06:01:47Z 发布,tag 指向 PR #74 合入的 `fd40701` = origin/main 头);v0.0.1 / v0.0.2 / v0.0.3 三版 Release 均存在且**非 draft 非 prerelease**;main 上 `rust.yml`(name=Rust)在精确 SHA `fd40701` 上 `conclusion=success`;tap 仓 `Casks/latermd.rb` 已自动跟版 **0.0.3**。
> **档位口径**(decisions-pending #53 拍板二):「失实」= 与已取证事实直接冲突;「滞后」= 内容不错但过时/应补,不混入失实。真机项一律保持未勾并标 **blocked_external**,不以自动结论冒充。
> 纠偏棒(V2)按本表行号(R/L 编号)只改失实处,每处改动旁注本表行号;本核对文档自身不改任何既有文件。

---

## 1. 事实基线:三版 Release 资产 vs distribution.md §2 命名表

| # | 核对项 | 实测(v0.0.1 / v0.0.2 / v0.0.3) | 与 §2 命名表 | 证据 |
|---|---|---|---|---|
| A1 | Release 存在性 | 三版均在,均非 draft 非 prerelease;published 依次 2026-09-26 / 09-29 / 09-30 | 一致 | E1 |
| A2 | Linux tar.xz | 三版均有 `latermd-x86_64-unknown-linux-gnu.tar.xz`(+`.sha256`) | 一致(无版本号,latest 热链可用) | E2/E3/E4 |
| A3 | Linux deb | **v0.0.1 无**(15 件);v0.0.2 起 `latermd_{version}_amd64.deb` 在 | 一致——deb job 2026-09-29 才接入(distribution.md 头注;commit `0519c01`/PR #56 同批引入),v0.0.1 无 deb 是**设计时序,不是资产缺失事故** | E2/E3/E4/E21 |
| A4 | macOS universal2 dmg | 三版均有 `latermd-v{version}-universal2-apple-darwin.dmg`(v0.0.1/v0.0.2/v0.0.3 各自内嵌版本号) | 一致 | E2/E3/E4 |
| A5 | macOS 每架构 tar.xz 与 source.tar.gz 清理 | **v0.0.2 / v0.0.3 确已不在**(10 件);**v0.0.1 仍在**(15 件含原料) | §2「发布后清理」对 v0.0.2+ 成立;v0.0.1 未清理非「清理步失败」——v0.0.1 时点的 macos-dmg.yml **尚无清理步**(grep 计数 0→4),清理步由 `0519c01`(PR #56)在 v0.0.1 之后引入 | E2/E3/E4/E18 |
| A6 | sha256.sum 含已删条目的既知代价 | v0.0.3 的 sha256.sum 共 6 行,其中 `latermd-aarch64-apple-darwin.tar.xz`、`latermd-x86_64-apple-darwin.tar.xz`、`source.tar.gz` 三条**资产已删而校验行仍在** | 与 §1「已知代价:sha256.sum 仍含已删条目,接受」逐字相符,**文档说法属实** | E5 |

## 2. acceptance-checklist §0 静态前置六项复核(ask 1c)

| 行 | 文档现状 | 事实证据 | 是否失实 | 建议改法 |
|---|---|---|---|---|
| §0.1(:17)dist-workspace.toml 五目标矩阵 ✅已核对 | 五 targets、`installers = []`、`merge-tasks = true` 均在;Win ARM64 原生 runner `windows-11-arm` | E14 | 否(属实) | 维持勾选 |
| §0.2(:18)release.yml 触发 ✅已核对 | tag 正则 `**[0-9]+.[0-9]+.[0-9]+*` 在(release.yml:47),workflow_dispatch LOCAL PATCH 在(:51);判据文本「tag 形如 v0.1.0」为格式示例 | E15 | 判据**滞后**(正则属实,v0.1.0 示例未按实际 tag 更新)→ 滞后档 L1 | 示例改 vX.Y.Z(实际 tag 为 v0.0.x) |
| §0.3(:19)macos-dmg.yml ✅已核对 | `on: release: types:[published]` + workflow_dispatch tag 入参在;产出名 `latermd-v{version}-universal2-apple-darwin.dmg` 在 | E16 | 否(属实) | 维持勾选 |
| §0.4(:20)cask 模板 ✅已核对(sha256 待 §6 填) | 模板结构与判据一致(url 插值模板 / livecheck :github_latest / `depends_on macos: :sonoma` / postflight);但 `version "0.1.0"`(latermd.rb:13)与 `sha256 :no_check`(:14)均已过时 | E20/E12/E13 | 结构属实;**version 行失实**(并入 R2);「sha256 待 §6 填」落点失效(并入 L3) | 见 R2 / L3 |
| §0.5(:21)六项门禁本地全绿 ✅每提交必跑 | 本棒仅抽验 `cargo fmt --all --check`(过,E19);全量六项未在本棒复跑 | E19 | 纪律属实;本棒**不冒充全量复跑** | 由编排收口在最终 head 复验六项后回填 |
| §0.6(:22)main 分支 CI 绿 ☐ 待你在 GitHub 上看一眼 | 结构化查询:name=Rust,status=completed,**conclusion=success** @ `fd40701`;Auto Tag / macOS dmg / Linux deb 在同 SHA 上单列均 success(不混判) | E6/E7/E8 | **已可自动核,勾选可回填** | 改 ✅ 并附 E6 命令与 SHA;该项从「人工看一眼」升级为可自动核项(#53 补全第 4 点) |

## 3. 失实行(16 处,R1–R16)

| 行号 | 文档现状 | 事实证据 | 是否失实 | 建议改法 |
|---|---|---|---|---|
| R1 | README.md:8「当前版本 **v0.0.1**(2026-09-26 首发)」 | Latest = v0.0.3(2026-09-30),三版均发 | 是 | 改「当前版本 v0.0.3(2026-09-30)」 |
| R2 | packaging/latermd.rb:13 模板 `version "0.1.0"` | v0.1.0 从未存在(E9 无此 tag);tap 真源已 "0.0.3"(E12) | 是 | 模板 version 改 0.0.3 或注明「实际版本由 tap auto-bump 维护,此处仅为初版模板」 |
| R3 | roadmap.md:20 P0 行「P0 剩余 = 首个 Release 发布(tag 触发 CI 全链路跑通)」 | 首个 Release 及后续两版均已发布,链路三版跑通(E1–E4) | 是 | 改「Release 链路已跑通(v0.0.1–v0.0.3 均自动发版);P0 剩余 = 三平台真机验收」 |
| R4 | roadmap.md:52「**待首个 tag 在 CI 上跑通验证**(本机无 macOS,lipo/hdiutil/codesign 均未自测)」 | dmg job 三版均成功产出并回传 dmg(E2–E4/E7) | 是 | 改为已跑通事实(2026-09-26 v0.0.1 首跑,后续两版复跑) |
| R5 | roadmap.md:54「剩余:上段打包的首跑验证,与 M0 两条真机项」 | 首跑验证已完成(E1–E4);M0 两条真机项(Win/mac IME、Win/mac wgpu)仍未做 | 是(前半句);真机部分属实 | 剩余只保留 M0 两条真机项 |
| R6 | p0-acceptance.md:16(复合行)「🟨 Linux 已本地验证;**Win / mac 待 CI 首跑**…**macOS dmg job 与 cask 模板待首个 tag 在 CI 验证**;**Windows 产物尚未在任何机器上跑过**」 | CI 五目标构建三版跑通,资产名单实证(E2–E4);dmg job 三版成功(E7);「Windows 产物尚未在任何机器上跑过」无反证 | 是(「待 CI 首跑」「dmg job 待验证」两处);「Windows 未上真机」保留 | **拆开处理**:CI 部分改真(五目标三版跑通);真机部分保留未勾,标 blocked_external(Win11/macOS 14 实机) |
| R7 | p0-acceptance.md:37「打包分发 🟨 配置就位,待首跑」 | 三版发版已跑通全链路(E1–E4) | 是 | 改 ✅(配置就位→三版跑通,补证据编号) |
| R8 | p0-acceptance.md:39「10 / 11 功能已落地,剩打包。」 | 打包已落地并发版三版 | 是 | 改「11 / 11 功能已落地」或改述为「打包链路三版跑通,余真机验收」 |
| R9 | p0-acceptance.md:59 §4 第 1 条「首个 tag 跑通发布链路:…`git tag v0.1.0 && git push origin v0.1.0`…」 | 手打 tag 从未发生:三版 tag 均为 annotated、tagger github-actions[bot]、message `Release vX (auto)`(E10);v0.1.0 从未存在(E9) | 是 | 按 #53 拍板五改写:发布链路已跑通为真(三版均 auto-tag 自动发版);手打命令降为 distribution.md §3.2 应急通道,版本号按实际写 |
| R10 | acceptance-checklist.md:30 §1 步骤「`git tag v0.1.0 && git push origin v0.1.0`」 | 同 R9;§1 整节「打 tag 触发发布」已成历史(自动链路 §1 of distribution.md 无人值守) | 是 | 按 #53 拍板五统一口径:常规=自动链路,手打=应急通道;逐字替换版本号解决不了定性问题,§1 需整节重新定性 |
| R11 | acceptance-checklist.md:40 §1 失败处置「`git tag -d v0.1.0 && git push --delete origin v0.1.0`」 | 同 R9/R10;失败处置命令本身仍有效,唯版本号示例失实且语境过时 | 是(语境失实) | 并入 §1 整节重定性;命令保留但按实际 tag 口径改写 |
| R12 | distribution.md:168 §3.3 勾选项「`latermd-v0.1.0-universal2-apple-darwin.dmg`」 | 实际资产名 `latermd-v0.0.{1,2,3}-universal2-apple-darwin.dmg`(E2–E4) | 是 | 改为 `latermd-v{version}-universal2-apple-darwin.dmg` 模板式;勾选清单按三版分别核对(A2–A5) |
| R13 | distribution.md:182「**windows-11-arm 首编能否通过是下一个待验项**」 | 三版资产名单均含 `latermd-aarch64-pc-windows-msvc.zip`(+`.sha256`),Win ARM64 首编已证通过(E2–E4) | 是 | 删该待验句,记「windows-11-arm 原生 runner 已实测通过(v0.0.1 起)」 |
| R14 | distribution.md:193-195 §3.4 第 1 点「**新建 `Casks/latermd.rb`**:从 packaging/latermd.rb 复制,改两处 —— `version` 填本次版本;`sha256` 用 step summary 的真值替换 `:no_check` 占位」 | tap 仓 `Casks/latermd.rb` **已存在**,version "0.0.3",sha256 已是真值 `d86ec30a…` 且注释明言由 tap auto-bump 自动维护(E12) | 是 | 改写为已落地事实(cask 已建、sha256 已由 auto-bump 维护);新建指引删除或标注仅存档 |
| R15 | distribution.md:202-209 §3.4 末段「auto-bump 空档(重要):…**只遍历 `Formula/`**,`Casks/` 不在自动范围…**cask 的 version/sha256 不更新 = 用户 brew 拿不到新版本**…不是可选项」 | tap auto-bump.yml 已含 `CASKS=("latermd:ailater/LaterMd" "lscreen:…")` 表,cron 每小时 :23,注释「Cask 的 url 是 v#{version} 插值模板永不改动,只更新 version + sha256 两行」(E13);cask 实际已自动跟版到 0.0.3(E12) | 是(前提已不真) | 按 #53 补全第 3 点改写:以 tap auto-bump.yml 的 CASKS 表为证,注明空档存在的历史时段与扩表动作;「发版后必做」的人工步骤相应撤销(#53 拍板四「保留现在时态」的旧口径因事实变更作废,这不是红线 1 担心的顺手历史化) |
| R16 | auto-plan.md:90 人工待办「~~三平台打包真机验收~~ 已过时:cargo-dist 链路已进 main,**v0.0.1/v0.0.2 均自动发版**」 | v0.0.3 亦自动发版(E1/E4/E10) | 是(漏 v0.0.3) | 补「v0.0.1–v0.0.3 均自动发版」 |

## 4. 滞后行(4 处,L1–L4,不混入失实)

| 行号 | 文档现状 | 事实证据 | 是否滞后 | 建议 |
|---|---|---|---|---|
| L1 | acceptance-checklist.md:18 §0.2 判据「tag 形如 `v0.1.0`」 | 正则属实(E15);仅示例版本号过时 | 是 | 示例改 vX.Y.Z |
| L2 | README.md:54/56 安装表 | macOS brew / Windows zip / Linux tar.xz 三条与实际发布形态一致(E12/E2–E4);**无 deb 渠道**一行,v0.0.2 起已有 `latermd_{version}_amd64.deb` 资产(E3/E4) | 是 | 安装表补 deb 一行(含 glibc ≥ 2.35 量级提示可复用 §4.2 口径) |
| L3 | acceptance-checklist.md:105–106 §7.1/§7.2(cask sha256 填 packaging/latermd.rb;推 tap) | sha256 现由 tap auto-bump 直读 Release asset digest 自动跟版(E12/E13),不再经 step summary → packaging/latermd.rb 手填;`brew install --cask` 真机实测仍未做 | 是(定性过时+真机部分未做) | §7.1 改述为「由 tap auto-bump 自动维护(源:asset digest)」;§7.2 的 brew 真机安装保持未勾,标 blocked_external |
| L4 | auto-plan.md:93 人工待办「cask version+sha256 手动回填(v0.0.2 起每版都要;#22 cask-bump 落地后自动化)」 | tap 侧已自动跟版(E12/E13),主仓 #22 cask-bump 与其功能重复 | 是(已失效) | 改述为「tap auto-bump 已自动维护 Casks;主仓 #22 与 tap 侧重复,撤与留留后续评审」(#53 补全第 3 点只登记冲突不代撤) |

## 5. 其余被点名核对行的结论(不失实,维持)

| 行号 | 文档说法 | 核对结论 | 证据 |
|---|---|---|---|
| roadmap.md:19 M0 行「验证 2 已实测(p50 60.3 fps, llvmpipe 下限)/验证 4 实测 O(n) 流式追加/出口仍卡两条真机项」 | 与 m0-report 证据一致,无新反证;验证 4 复测在队列 #46,验证 1 候选框跟随挂 #19 | 属实,维持;复测后按事实订正 | — |
| p0-acceptance.md:18「`latermd-export` 9 项测试」 | 本会话实跑 `cargo test -p latermd-export`:`running 9 tests` / `9 passed; 0 failed`,自述与实跑一致(#53 拍板三:计数以实跑输出为准) | 属实 | E22 |
| distribution.md §1 链路描述(auto-tag → release → dmg/deb dispatch、防递归、LOCAL PATCH、allow-dirty) | 与配置文件逐项相符(E14/E15/E16/E21);三版发版时序与链路吻合(E7) | 属实 | E7/E14/E15/E16/E21 |
| distribution.md §2 命名表 | 与三版实际资产逐项对上(A1–A6) | 属实 | E2–E4 |

## 6. 真机项(blocked_external,保持未勾,禁止自动冒充)

以下各项自动侧**只做了有界替代或完全未做**,验收勾选保持 ☐,统一标 blocked_external:

- **IME 三判据**(候选框跟随/不吞字/不抢焦点):Win11 微软拼音(checklist §2.4)、macOS 14 简体拼音(§3.4);Linux fcitx5 候选框跟随挂队列 #19(checklist §4.3)。
- **wgpu 真机 adapter**:Win11 DX12(§2.2)、macOS Metal(§3.3)。
- **装包交互**:Win SmartScreen「仍要运行」(§2.1)、mac Gatekeeper + `xattr` 直下 dmg(§3.2)、dmg 内 .app 真机启动、universal2 双架构各跑一次(§3.5)。
- **brew 真机安装**:`brew install --cask crazykun/ailater/latermd` 实测通过(§7.2 后半;§7.2 前半「推 tap」已由 tap 侧自动跟版取代,L3)。
- **稳定性长跑**:连续写 1 小时(§5.1)——本棒未做任何长跑,自动侧无替代;同帧换肤(§5.2)/切模式不丢光标(§5.3)同属真机段。
- **外壳拖拽手感**(§8.2 真机部分):xdotool 合成输入不可信已记档,留人工。
- **图床真机上传走通一次**(§6 相关项;auto-plan.md:94)。
- **Windows CRLF 实机往返**(p0-acceptance.md:19「未做:Windows CRLF 实机往返」;file.rs 字节往返的 Linux 侧测试已有)。
- **Win/mac 产物上真机跑**(p0-acceptance.md:16 保留部分,R6)。
- **m0-report Win/mac 真机项**(IME、字体 face index、adapter 上报)。

## 7. 回填状态小结(p0-acceptance 四条验收标准,ask 2)

| p0-acceptance §1 行 | V1 结论 |
|---|---|
| 验收 1 三平台可安装(:16) | 拆开:CI 产物链路 ✅(三版五目标资产实证);真机安装/启动 ☐ blocked_external(R6) |
| 验收 2 连续写 1 小时(:17) | 维持 🟨:性能证据在(llvmpipe 下限口径);长跑人工未做,blocked_external |
| 验收 3 导出 HTML 可交付(:18) | ✅ 逻辑已测属实(9 passed,E22);「真实浏览器观感确认」仍未做,如实保留 |
| 验收 4 .md 原样(:19) | ✅ 字节往返测试属实;Windows CRLF 实机往返仍未做,blocked_external |

---

## 证据摘录(命令 + 关键输出)

> 全部命令 2026-09-30 本会话实跑;gh 侧仅 list/view/api GET,git 侧仅只读子命令。

- **E1 Release 列表**
  `gh release list --repo ailater/LaterMd --limit 10 --json tagName,isDraft,isPrerelease,publishedAt`
  → `[{"tagName":"v0.0.3","isDraft":false,"isPrerelease":false,"publishedAt":"2026-09-30T06:01:47Z"},{"tagName":"v0.0.2",…,"publishedAt":"2026-09-29T01:34:43Z"},{"tagName":"v0.0.1",…,"publishedAt":"2026-09-26T10:30:56Z"}]`
- **E2 v0.0.1 资产(15 件)**
  `gh release view v0.0.1 --repo ailater/LaterMd --json tagName,isDraft,isPrerelease,publishedAt,assets --jq '{…,n:(.assets|length),names:[.assets[].name]}'`
  → n=15;含 `latermd-v0.0.1-universal2-apple-darwin.dmg`、五目标 dist 资产(+`.sha256`)、`dist-manifest.json`、`sha256.sum`;**无 deb**;含未清理的 `latermd-{aarch64,x86_64}-apple-darwin.tar.xz`(+`.sha256`)与 `source.tar.gz`(+`.sha256`)
- **E3 v0.0.2 资产(10 件)** 同法 → 含 `latermd-v0.0.2-universal2-apple-darwin.dmg`、`latermd_0.0.2_amd64.deb`、五目标 dist 资产、`dist-manifest.json`、`sha256.sum`;mac 每架构 tar.xz 与 source.tar.gz 已不在
- **E4 v0.0.3 资产(10 件)** 同法 → 同构 v0.0.2,`latermd-v0.0.3-universal2-apple-darwin.dmg`、`latermd_0.0.3_amd64.deb` 在
- **E5 sha256.sum 实读(v0.0.3)**
  `gh api repos/ailater/LaterMd/releases/tags/v0.0.3 --jq '.assets[] | select(.name=="sha256.sum") | .id'` → 600168338
  `gh api -H "Accept: application/octet-stream" repos/ailater/LaterMd/releases/assets/600168338`
  → 6 行,其中 3 行对应**已删除**资产:`*latermd-aarch64-apple-darwin.tar.xz`、`*latermd-x86_64-apple-darwin.tar.xz`、`*source.tar.gz`
- **E6 main CI 结构化查询(rust.yml,精确 SHA)**
  `gh run list --repo ailater/LaterMd --workflow rust.yml --branch main --commit fd40701a1fde4263c698846f3a2dbcade0d49959 --json name,status,conclusion`
  → `[{"conclusion":"success","name":"Rust","status":"completed"}]`(只认 name=Rust)
- **E7 main 最近 run(其他 workflow 单列)**
  `gh run list --repo ailater/LaterMd --branch main --limit 8 --json workflowName,status,conclusion,headSha,event,createdAt`
  → @fd40701:Linux deb(dispatch)success、macOS dmg(dispatch)success、Auto Tag(workflow_run)success、Rust(push)success;@c0eed4e/@c1b8ca1:Auto Tag + Rust 均 success
- **E8 tag 与 main 头互证**
  `git rev-parse origin/main` → `fd40701a1fde4263c698846f3a2dbcade0d49959`(Merge pull request #74 from ailater/chore/bump-v0.0.3)
  `git cat-file -p v0.0.3` → object `fd40701…` = origin/main 头
- **E9 tag 清单与远端**
  `git tag -l`(v0.0.1/v0.0.2/v0.0.3 在,**无 v0.1.0**);`git ls-remote --tags origin`(三版 refs/tags/v0.0.x 与 `^{}` 解引用均在远端)
- **E10 三版 tag 对象形态**
  `git cat-file -p v0.0.1 / v0.0.2 / v0.0.3` → 均 annotated,type commit,tagger `github-actions[bot]`,message `Release vX (auto)`,object 依次 `daf55fb…` / `fe15e34…` / `fd40701…`
- **E11 版本号互证**
  `Cargo.toml:20` → `version = "0.0.3"`;`CHANGELOG.md` → `:12 ## v0.0.3 - 2026-09-30`、`:53 ## v0.0.2 - 2026-09-29`、`:109 ## v0.0.1 - 2026-09-26`;`git show v0.0.1:Cargo.toml` → `version = "0.0.1"`、`git show v0.0.3:Cargo.toml` → `version = "0.0.3"`
- **E12 tap 仓 cask 实况**
  `gh api repos/crazykun/homebrew-ailater/contents/Casks/latermd.rb --jq '.content' | base64 -d`
  → `version "0.0.3"`;`# sha256 由 tap 仓库 auto-bump workflow 自动维护(源:Release asset digest)`;`sha256 "d86ec30ac669fcc563db5e3e1b4d234eb5b9036731dce107b8222ddb06eeba98"`;url 模板 `latermd-v#{version}-universal2-apple-darwin.dmg`、livecheck `:github_latest`、`depends_on macos: :sonoma`、postflight xattr
- **E13 tap auto-bump.yml CASKS 表**
  `gh api repos/crazykun/homebrew-ailater/contents/.github/workflows/auto-bump.yml --jq '.content' | base64 -d`
  → `cron: "23 * * * *"`;`CASKS=( "latermd:ailater/LaterMd" "lscreen:crazykun/LaterScreen" )`;注释「Cask 的 url 是 v#{version} 插值模板永不改动,只更新 version + sha256 两行;sha256 从 GitHub Release asset 的 digest 字段直接读取(免下载)」
- **E14 dist-workspace.toml** → `:14 allow-dirty = ["ci"]`、`:23 installers = []`、`:26-29+38` 五 targets + `merge-tasks = true`、`:45 runner = "windows-11-arm"`
- **E15 release.yml 触发** → `:43-51 on: pull_request / push tags '**[0-9]+.[0-9]+.[0-9]+*' / workflow_dispatch`(LOCAL PATCH 注释:GITHUB_TOKEN 推 tag 不触发,补 dispatch 入口);`permissions` 含 `"actions": "write"`
- **E16 macos-dmg.yml** → `:15 产出资产名:latermd-v{version}-universal2-apple-darwin.dmg`;`:20-22 on: release types:[published]` + workflow_dispatch tag 入参
- **E17 rust.yml** → `:1 name: Rust`、`:7 branches: ["main"]`、`:19 Gate (fmt / clippy / test / doc)`、`:25 dtolnay/rust-toolchain@1.98.0`、`:36-52` fmt --check / 三轮 clippy(-D warnings)/ cargo test --workspace --all-features / cargo doc
- **E18 清理步时序**
  `git show v0.0.1:.github/workflows/macos-dmg.yml | grep -c 'delete-asset\|清理'` → **0**;v0.0.3 时点同命令 → **4**;`git log --oneline -S 'delete-asset' -- .github/workflows/macos-dmg.yml` → `0519c01 chore(release): Linux x64 deb 自建 job + Release 冗余资产发布后清理 (#56)`
- **E19 fmt 抽验** → `cargo fmt --all --check` 通过(无输出)。全量六项门禁本棒未复跑,由编排收口在最终 head 复验
- **E20 cask 模板现状** → packaging/latermd.rb:13 `version "0.1.0"`、:14 `sha256 :no_check # TODO: 首个 Release 发布后填…`
- **E21 workflows 清单** → `.github/workflows/`:auto-tag.yml、linux-deb.yml、macos-dmg.yml、release.yml、rust.yml(distribution.md 头注五份引用齐备)
- **E22 latermd-export 测试实跑** → `cargo test -p latermd-export`:`running 9 tests` / `test result: ok. 9 passed; 0 failed`(与 p0-acceptance.md:18 自述一致;计数以实跑输出为准,#53 拍板三)

---

## 8. V3 自动验收与 blocked_external 登记(2026-09-30)

> 本章由 #44 V3 棒产出:本棒**实跑**的自动验收证据(编号 **V3-E1…**)、三分栏汇总与人工项挂账。两套证据编号(E1–E22 = V1 取证;V3-E… = 本棒)不混用。
> 证据平台:**Linux / Deepin rolling + X11(DISPLAY=:0)**。按 #53 拍板三:Linux 自动结论不冒充三平台结论,凡引用处注明平台。
> workspace 全量六项门禁本棒未复跑(与 V1 §0.5 同口径),由编排收口在最终 head 复验;本棒只跑验收点名的相关测试目标。

### 8.1 三分栏 · 自动完成(本棒实跑)

| 验收项 | 命令 | 结果 | 证据 |
|---|---|---|---|
| `.md` 不篡改——导出逻辑 | `cargo test -p latermd-export` | `test result: ok. 9 passed; 0 failed`(骨架/内嵌 CSS/代码块 language class/GFM 表格/任务列表/标题转义/删除线/脚注/空输入) | V3-E1 |
| `.md` 不篡改——打开-保存往返 | `cargo test -p latermd-app file::` | `test result: ok. 6 passed; 0 failed`,含 `file::tests::write_then_read_is_byte_exact`(CRLF/LF 混排+尾随空行**逐字节一致**;既有测试引用,不重造) | V3-E2 |
| 导出 HTML 可读(结构断言) | 生成样例 [sample-export-p0.html](sample-export-p0.html) + 12 项断言 | 全过:非空(2223 B)、`<!DOCTYPE html>` 开头 `</html>` 结尾、中文正文原样、`<title>`、表格/`language-rust` 代码块/checkbox/`<del>`/脚注、无 U+FFFD、内嵌 CSS 且无 `<link>`/`<script>` 外链 | V3-E3 |
| Linux 本机有界冒烟 | `DISPLAY=:0 timeout 15 cargo run -p latermd-app --quiet` | **无 panic**:rc=124(timeout 到期杀,预期),stdout/stderr 均 0 字节;补充窗口取证(6 秒段)`WM_CLASS("", "LaterMD")`、`_NET_WM_NAME = "LaterMD — 未命名"`、900×600 @ (40,21) | V3-E4 |
| checklist §0.6 main CI 绿(本棒复验) | `gh run list --repo ailater/LaterMd --workflow rust.yml --branch main --commit fd40701a1fde4263c698846f3a2dbcade0d49959 --json name,status,conclusion` | `[{"conclusion":"success","name":"Rust","status":"completed"}]`(只认 name=Rust,与 V1 E6 一致) | V3-E5 |

三条如实声明:
- V3-E3 是**结构断言**(非空/含正文/骨架完整),不冒充「人眼可读性」——真实浏览器观感确认仍待人工(8.2)。
- V3-E4 是 **15 秒有界冒烟**(任务 c 口径),不冒充验收 2 的「连续写 1 小时」长跑;且是 debug build `cargo run`,非 dist 产物 tar.xz 解压运行(后者历史实测见 p0-acceptance §1 验收 1)。
- 任务原文 `cargo run -p latermd` 的包名在本仓不存在:workspace 包名是 `latermd-app`(产物 binary 名 `latermd`),已按现状以 `-p latermd-app` 执行。

### 8.2 三分栏 · 待人工(不被外部资源阻塞,需要人做)

| 项 | 缺什么 | 谁能补 |
|---|---|---|
| 导出 HTML 浏览器观感确认 | 人在真实浏览器打开 [sample-export-p0.html](sample-export-p0.html) 看一眼(结构断言已过,观感不可自动) | 坤哥 |
| Linux X11 IME 候选框跟随 | #19 落地后 fcitx5 目视复验(m0-report 验证 1 只记复测不销账) | 坤哥(自动段挂队列 #19) |
| 外壳拖拽 / resize 手感(checklist §8.2 真机部分) | 真窗口手感;xdotool 合成输入不可信已记档(m5-acceptance §0) | 坤哥 |
| checklist §6 六项真机抽查 | 皮肤文件 / 跟随系统 / Live Preview / wikilink / 大纲预览跳转 / MCP 被外部调用的人工操作 | 坤哥 |

### 8.3 三分栏 · 被阻塞(blocked_external,保持未勾,逐项挂账)

| 项 | 缺什么 | 谁能补 | 挂账处 |
|---|---|---|---|
| Win11 安装 + IME 真机(§2.1/§2.2/§2.4) | Win11 实体机:SmartScreen 交互、DX12 adapter 上报、微软拼音三判据(跟随/不吞字/不抢焦点) | 坤哥 | checklist §2 |
| macOS 14 安装 + IME 真机(§3.1–§3.5) | macOS 14 (Sonoma) 实体机:`.app` 安装、Gatekeeper/xattr、Metal adapter、简体拼音三判据、universal2 双架构各跑一次 | 坤哥 | checklist §3 |
| wgpu 真机启动(adapter 上报) | 同上两台真机(Linux 侧 llvmpipe 已跑通,不冒充 DX12/Metal) | 坤哥 | checklist §2.2/§3.3、m0-report 验证 3 |
| `brew install --cask crazykun/ailater/latermd` 实测 | macOS 真机 + brew 环境;主仓 #22 自动回填链另需 `HOMEBREW_TAP_TOKEN` 进 secrets——tap 侧 auto-bump 已自动跟版 0.0.3(E12/E13),brew 实测本身不依赖该 token | 坤哥 | checklist §7.2 后半 |
| 连续写 1 小时长跑(§5.1–§5.3) | 人工连续写作时段 + 真实文档场景;自动侧只做 15s 有界冒烟(V3-E4),不冒充 | 坤哥 | checklist §5 |
| 图床真机上传走通一次 | 自定义图床服务 + 真实凭据 + 真机操作。注:acceptance-checklist 本身**无图床独立行**(§6 六项不含图床),实际挂账在 auto-plan 人工待办末行「图床真机上传走通一次(acceptance-checklist)」与 roadmap「图片框与图床」行的真机验收段 | 坤哥 | auto-plan 人工待办、roadmap |
| Windows CRLF 实机往返 | Windows 实体机(file.rs 字节往返的 Linux 侧测试已有,V3-E2;Win 侧 Notepad/编辑器链路的 CRLF 行为需实机) | 坤哥 | p0-acceptance §1 验收 4 |

### 8.4 证据摘录(V3 本棒实跑,2026-09-30,Linux/Deepin X11)

- **V3-E1 导出测试(既有)**
  `cargo test -p latermd-export`
  → `running 9 tests` … `test result: ok. 9 passed; 0 failed; 0 ignored`(9 项逐一 `ok`:empty_input…/minimal_css…/inline_code…/heading…/fenced_code…/gfm_table…/title_falls_back…/task_list…/strikethrough…)
- **V3-E2 保存/往返测试(既有)**
  `cargo test -p latermd-app file::`
  → `running 6 tests` … `test result: ok. 6 passed; 0 failed; 406 filtered out`
  6 项含 `file::tests::write_then_read_is_byte_exact … ok`(该测试断言 `read()==原文` 且 `std::fs::read()==原文.as_bytes()`,crates/latermd-app/src/file.rs:189)
- **V3-E3 样例 HTML 生成与结构断言**
  生成(临时 probe 链 `latermd_export::export_html`,不触碰仓库代码;输入样例含标题/中文正文/表格/代码块/任务列表/删除线/脚注/引用):
  `cargo build -p latermd-export --quiet` + `rustc --edition 2024 /tmp/export_probe.rs --extern latermd_export=target/debug/liblatermd_export.rlib -L dependency=target/debug/deps -o /tmp/export_probe` + `/tmp/export_probe > docs/sample-export-p0.html`
  → `wc -c` = 2223;`head -c 300` 显示 `<!DOCTYPE html>`/`<title>LaterMD P0 验收样例</title>`/内嵌 CSS。
  12 项断言(非空>1KB / DOCTYPE 开头 / `</html>` 结尾 / 含「双栏预览」正文 / `<title>` 命中 / `<table>` / `language-rust` / checkbox / `<del>` / footnote / 无 U+FFFD / 内嵌 CSS 且无 `<link>`/`<script>` 外链)输出 `PASS` ×12,`=== 断言结果: 全部通过 ===`
- **V3-E4 有界冒烟(Linux/X11)**
  `DISPLAY=:0 timeout 15 cargo run -p latermd-app --quiet > /tmp/smoke_stdout.log 2> /tmp/smoke_stderr.log`
  → `exit_code=124`(timeout 到期);stdout 0 bytes、stderr 0 bytes;`grep -iE 'panicked|RUST_BACKTRACE'` 无命中;无残留进程。rc=124 本身证明进程活满 15 秒(启动即 panic 的退出码是 101,不会等满)。
  窗口取证(第二次运行,6 秒段,SIGTERM 收尾 rc=143):
  `xdotool search --onlyvisible --class latermd` → 窗口 341835780;`xprop -id 341835780 WM_CLASS _NET_WM_NAME` → `WM_CLASS(STRING) = "", "LaterMD"`、`_NET_WM_NAME(UTF8_STRING) = "LaterMD — 未命名"`;`getwindowgeometry` → `Geometry: 900x600` @ (40,21);期间 stderr 全空(无 wgpu/panic 报错)。
- **V3-E5 main CI 结构化复验(本棒)**
  `SHA=$(git rev-parse origin/main)` → `fd40701a1fde4263c698846f3a2dbcade0d49959`
  `gh run list --repo ailater/LaterMd --workflow rust.yml --branch main --commit "$SHA" --json name,status,conclusion,headSha`
  → `[{"conclusion":"success","headSha":"fd40701…","name":"Rust","status":"completed"}]`(仅 name=Rust 计入判据;与 V1 E6 同 SHA 同结论)
