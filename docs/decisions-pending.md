# 待用户决策记录（decisions-pending）

> 自动开发循环遇到「本该问用户」的岔路口时，在这里登记：**岔路是什么、自动选了什么、为什么、想改怎么改**。
> 选择由循环自行做出并继续执行，不阻塞；用户事后翻此文件，按「如何改」一节操作即可推翻。
> 编号 #6 为当前阻塞项，需用户裁决。

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
