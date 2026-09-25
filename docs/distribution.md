# 分发与发布 runbook(cargo-dist + GitHub Release + Homebrew cask)

> 本文是发布操作的**唯一事实来源**。配置侧的事实源分别是:
> [dist-workspace.toml](../dist-workspace.toml)(dist 目标矩阵)、
> [.github/workflows/release.yml](../.github/workflows/release.yml)(dist 生成,**禁止手改**)、
> [.github/workflows/macos-dmg.yml](../.github/workflows/macos-dmg.yml)(自建 dmg job)、
> [packaging/latermd.rb](../packaging/latermd.rb)(cask 模板)。
> 与本文冲突时,以配置文件为准并回改本文。

## 1. 发布链路总览

```
git tag v0.1.0 && git push origin v0.1.0
        │
        ▼  on: push: tags: '**[0-9]+.[0-9]+.[0-9]+*'   (release.yml:45)
┌─────────────────────────────────────────────┐
│ release.yml(cargo-dist 0.33.0 生成)          │
│ plan → build-local(4 job,5 目标)             │
│   macos-14:双 darwin target 单 job(merge-tasks)│
│   ubuntu-22.04:linux x64                     │
│   ubuntu-22.04 + cargo-xwin 容器:win ARM64   │
│   windows-2022:win x64                       │
│ → host:gh release create + 上传全部资产        │
└─────────────────────────────────────────────┘
        │  Release 状态变 published
        ▼  on: release: types: [published]        (macos-dmg.yml:19)
┌─────────────────────────────────────────────┐
│ macos-dmg.yml(自建 job,macos-14)             │
│ 下载双架构 tar.xz → lipo 合一 → 组装 .app      │
│ → hdiutil 合 dmg → sha256 写 step summary     │
│ → gh release upload 回传同一 Release           │
└─────────────────────────────────────────────┘
```

- **tag 格式**:`v0.1.0` / `latermd/0.1.0` / `releases/v1.0.0` 均可触发(模式
  `**[0-9]+.[0-9]+.[0-9]+*`);带 `-beta.1` 等预发布后缀时 GitHub Release 自动标记
  prerelease。tag 必须是三段 SemVer。
- **PR 也会触发 release.yml**,但默认 `pr-run-mode = "plan"` 只跑 dist plan 校验,
  不构建不发布,与「CI 只在 main 跑」(rust.yml)不冲突。
- **CI 工具链**:release.yml 不安装任何指定版本工具链,rustup 由仓库根
  `rust-toolchain.toml`(channel = "1.98.0")接管——AGENTS.md §4 的钉法就是
  这个文件,**不要**给 dist 配置加 `rust-toolchain-version`(已弃用)。
- **产物不含 glow**:所有构建只走默认 feature(wgpu)。`LATERMD_RENDERER=glow`
  仅是运行时逃生口(AGENTS.md §5)。

## 2. 资产命名规范(钉死,不随版本变格式)

| 资产 | 产生方 | 命名 | 版本号 |
|---|---|---|---|
| Linux | dist | `latermd-x86_64-unknown-linux-gnu.tar.xz` | **无**(官方刻意,`releases/latest/download/` 热链可用) |
| macOS x64 | dist | `latermd-x86_64-apple-darwin.tar.xz` | 无 |
| macOS arm64 | dist | `latermd-aarch64-apple-darwin.tar.xz` | 无 |
| Windows x64 | dist | `latermd-x86_64-pc-windows-msvc.zip` | 无 |
| Windows ARM64 | dist | `latermd-aarch64-pc-windows-msvc.zip` | 无 |
| macOS universal2 | macos-dmg.yml | `latermd-v{version}-universal2-apple-darwin.dmg` | **有**(lscreen 同构,cask url 模板依赖) |
| 校验和 | dist | 每资产附 `.sha256`,另有 `sha256.sum` | — |

- 二进制名统一 `latermd`(crate 名仍为 `latermd-app`,`[[bin]]` 改名 +
  `crates/latermd-app/dist.toml` shadow 资产前缀)。
- **风险登记册 #7**:cask 停更的教训是 dmg 资产命名变化后 cask URL 模板没跟上
  (lscreen 曾因此停在 0.6.0)。**任何资产命名调整必须同步改 cask url / livecheck**,
  并在本文表 2 回改。
- 注意 dist 的 tar.xz/zip 与自建 dmg 的版本号策略**刻意不同**:前者无版本号
  (dist 官方设计,支持 latest 热链),后者带版本号(Homebrew cask 惯例,url 与
  version 绑定以便 sha256 校验)。这是两套命名共存的原因,不是疏漏。

## 3. 首个 Release 步骤(v0.1.0)

### 3.1 前置检查

1. 打包 PR 已合入 `main`(分支保护要求 PR;release.yml 在 tag push 时从 tag
   指向的 commit 构建)。
2. 本地六项门禁全绿(AGENTS.md §8;vendor 改动另跑 check.sh)。
3. 配置核对(改过则先验证再打 tag):

   ```bash
   dist manifest --output-format=json \
     --target=aarch64-apple-darwin --target=aarch64-pc-windows-msvc \
     --target=x86_64-apple-darwin --target=x86_64-unknown-linux-gnu \
     --target=x86_64-pc-windows-msvc
   ```

   确认矩阵含 4 个 build job(双 darwin 合并在 macos-14)、资产名与表 2 一致。
4. `Cargo.toml` 的 `workspace.package.version` 已是待发版本(dist 要求 tag 版本
   与 workspace 版本一致,否则报版本不匹配)。

### 3.2 发布

```bash
git checkout main && git pull --rebase origin main
git tag v0.1.0 && git push origin v0.1.0
```

之后不需要也不应该手动跑任何 workflow:release.yml 建好 Release 并上传
5 个 dist 资产后,macos-dmg.yml 自动被 `release: published` 事件触发,合成
universal2 dmg 回传。全程约 20–40 分钟(Windows xwin 交叉编最慢)。

### 3.3 资产核对清单(Release 页面逐项勾)

- [ ] `latermd-x86_64-unknown-linux-gnu.tar.xz`(+`.sha256`)
- [ ] `latermd-x86_64-apple-darwin.tar.xz`(+`.sha256`)
- [ ] `latermd-aarch64-apple-darwin.tar.xz`(+`.sha256`)
- [ ] `latermd-x86_64-pc-windows-msvc.zip`(+`.sha256`)
- [ ] `latermd-aarch64-pc-windows-msvc.zip`(+`.sha256`)
- [ ] `latermd-v0.1.0-universal2-apple-darwin.dmg`(macos-dmg job 完成)
- [ ] `sha256.sum`、`source.tar.gz` 及 `dist-manifest.json`(dist 附带)
- [ ] 「macOS dmg」job 的 **step summary** 里有 dmg 的 sha256 —— 首次填入
      cask 模板用(macos-dmg.yml:96-106)。
- [ ] Release 未被误标 prerelease(除非 tag 带预发布后缀)。

首个 Release 还要额外验证(只此一次,之后信任链路):
- Windows xwin 交叉编能否通过(egui/wgpu 栈未经 xwin 实测;若失败,改用
  `github-custom-runners` 把 `aarch64-pc-windows-msvc` 指到 `windows-11-arm`
  原生 runner 后重新 generate);
- dmg 内 .app 在真机可启动(本机无 macOS,lipo/hdiutil/codesign 均未自测)。

### 3.4 cask 落地(动 tap 仓库 crazykun/homebrew-ailater)

模板在本仓 [packaging/latermd.rb](../packaging/latermd.rb),已按 lscreen 模式
写好(universal2 单 dmg url、`livecheck :github_latest`、postflight 去
quarantine、`depends_on macos: :sonoma`、zap)。**复刻对象是
`Casks/lscreen.rb`,不要抄同 tap 的 `glmeter.rb`**(后者仍是按架构拼 URL 的
旧式双包写法,lscreen v0.8.0 起已废弃该模式)。要动三处:

1. **新建 `Casks/latermd.rb`**:从 packaging/latermd.rb 复制,改两处 ——
   `version` 填本次版本;`sha256` 用 step summary 的真值替换 `:no_check`
   占位(`:no_check` 只是让 cask 在填值前可安装,不是长期形态)。
2. **tap README**:Formula/Cask 表加 `latermd` 一行,安装命令区补
   `brew install --cask crazykun/ailater/latermd`。
3. **验证**:`brew install --cask crazykun/ailater/latermd` 在真机跑通;随后
   主仓 README「安装(发布后)」的渠道预告注删掉并回填实测过的命令。

**auto-bump 空档(重要)**:tap 的 auto-bump 流水线(cron 每小时 :23,
`bump_formula.py` 做 version/url/sha256 三点重写)**只遍历 `Formula/`**,
`Casks/` 不在自动范围 —— lscreen 的 cask 停在 0.6.0 就是手动维护失手的实证。
LaterMD 若只发 cask,每次发版后需手动改 `Casks/latermd.rb` 的
version/sha256;要自动化,需扩 `bump_formula.py` 支持 Casks(其 url+相邻
sha256 的正则替换逻辑对 cask 同样适用),或在 tap 的 FORMULAS 表加上
`latermd:LaterMd` 前先确认流水线已扩。**cask 的 version/sha256 不更新 =
用户 brew 拿不到新版本**,这是发版检查单的一部分,不是可选项。

### 3.5 首发后回填

- 主仓 README 安装节:删「渠道预告」免责,确认三条路径命令可执行。
- docs/roadmap.md「当前位置」:P0 打包条目状态更新。
- m0-report.md 真机项:Win11 / macOS 冒烟结果(IME、字体 face index 核对)。

## 4. 后续版本发布(v0.1.1+)

1. 改 `workspace.package.version`(workspace 内 dist-able crate 版本必须一致,
   lockstep)。
2. 常规 PR 合入 main,打 tag `v0.1.1` push —— 链路同 §3.2。
3. **发版后必做**:更新 tap 的 `Casks/latermd.rb`(version + sha256,取新
   Release 的 dmg step summary 值;见 §3.4 的 auto-bump 空档)。
4. dist 配置(dist-workspace.toml)改动后本地必须重跑 §3.1 第 3 步的
   manifest 校验,再 `dist generate` 重新生成 release.yml(手改 release.yml
   会被 dist 视为 error 并在下次 generate 时覆盖)。

## 5. 无签名路线的用户侧影响(README「安装」节的依据)

- **Windows**:SmartScreen 警告 → 「更多信息 → 仍要运行」。视为已知门槛,
  不修(无证书路线,AGENTS.md §5)。
- **macOS + brew cask**:postflight 自动 `xattr -dr com.apple.quarantine`,
  用户无感。
- **macOS 直下 dmg**:Gatekeeper「已损坏,无法打开」/「无法验证开发者」→
  `sudo xattr -dr com.apple.quarantine /Applications/LaterMD.app`。
- ad-hoc 签名(codesign -s -,macos-dmg.yml)**不解决** Gatekeeper,只为避免
  无签名 bundle 的启动异常;信任门槛完全靠 cask postflight / 手动 xattr。
