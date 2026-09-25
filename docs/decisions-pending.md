# 待用户决策记录（decisions-pending）

> 自动开发循环遇到「本该问用户」的岔路口时，在这里登记：**岔路是什么、自动选了什么、为什么、想改怎么改**。
> 选择由循环自行做出并继续执行，不阻塞；用户事后翻此文件，按「如何改」一节操作即可推翻。
> 编号 #6 为当前阻塞项，需用户裁决。

## #6 ⚠️ 阻塞：双会话并发冲突（需用户裁决，循环已暂停）

- **事实**：2026-09-25 上午，本自动循环与另一个活跃会话（UI 设计文档线，分支 `feature/ui-design-doc`；此前打包线也在此工作区）**共享同一工作目录**，已实际互相踩踏：另一会话两次切换分支/恢复 WIP，导致自动循环第一棒（run `dwfrun-fade3872`，P0 收尾修补）四次尝试均卡在 `git checkout main`，最终被主动停止（stop_reason=model，可恢复；实现者尚未写入代码，工作区无半截污染）。
- **为什么不能自动选**：停掉另一会话或抢占工作区会损害用户可能正在用的会话；这不是功能取舍，是资源冲突。
- **选项**：
  1. 等另一会话完工后回复「继续循环」——看护将 Resume `dwfrun-fade3872`。
  2. 给自动循环独立 worktree（`git worktree add ../LaterMD-auto main`，脚本改为 git -C / cargo --manifest-path 在 worktree 内操作）——一劳永逸消除互踩。**推荐**。
  3. 放弃全自动循环，回到人工逐棒启动。
- **现状**：第一棒 stopped 待命；新看护 `automation-97453eb0`（每5分钟自动开发循环看护）运行中，检测到 stopped/model 只报一行不动；旧看护 `automation-5b29bca9` 在 API 列表消失（一次超时的更新所致），若 Automations 页面仍显示请手动删除。

## #1 打包工作的合并（已解决）

- **原岔路**：打包工作在 `feature/p0-packaging` 分支，自动循环是否代为合并。
- **结果**：另一会话已走正规 PR 流程合并——PR #7（`feature/p0-packaging`：cargo-dist、universal2 dmg、release workflow、cask 模板）与 PR #8（`feature/m0-perf-bench`：长文档 bench）均已合入 main（`f822b61`）。剩余真机验收（打 tag 看 Release、brew 装机）仍属人工。

## #2 直推 main 豁免（自动循环专用）

- **岔路**：AGENTS.md §8 要求 main 走 PR + 分支保护，禁止直推。
- **自动选择**：自动循环的 workflow 直接在 main 上 commit 并 push（2026-09-25 用户指令「每个 workflow 完成后，push 推送」）。
- **如何改**：在仓库设置开 main 分支保护（require PR），循环 push 将失败并记录，届时改为推 `auto/<功能名>` 分支 + 提示人工合并。

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
