#!/usr/bin/env bash
# Twemoji 72x72 彩色资产钉版本下载脚本(可复现、可审计,#47 A1)。
#
# 手法沿用 .zcode/workflow-drafts/emoji-color-probe/fetch-assets.sh:
# 从 jdecked/twemoji v17.0.3(上游 twitter/twemoji 停更后的社区延续 fork)
# 的 assets/72x72/ 逐枚下载 PNG。清单是 272 枚钉死文件名,派生自
# crates/latermd-app/src/ui/emoji_data.rs 同一提交(规则:字符逐码位小写
# hex 以 "-" 相连;Twemoji 约定序列里的 FE0F 不进文件名,如 ✌️ -> 270c.png;
# 旗帜为双区域指示码位,如 🇨🇳 -> 1f1e8-1f1f3.png)。emoji_data.rs 若增删
# 条目,须按同一规则重生成清单并同步 EXPECTED_COUNT,不得盲改。
#
# 用法:bash fetch.sh          # 重下全部并自校验(幂等,覆盖已存在文件)
#       bash fetch.sh --verify # 只校验不下载(断言 272 枚存在/非空/合法 PNG)
# 网络不可达时 curl 失败,缺失枚由校验段如实列出;缺枚单元在面板回落
# 黑白字形(见可行性调查 §2.3),不阻塞构建。
set -euo pipefail

TAG=v17.0.3
BASE="https://raw.githubusercontent.com/jdecked/twemoji/${TAG}/assets/72x72"
EXPECTED_COUNT=272
DEST="$(cd "$(dirname "$0")" && pwd)"

# 钉死清单:272 枚,顺序 = emoji_data.rs 表序(派生规则见文件头)。
NAMES=(
  1f600.png
  1f603.png
  1f604.png
  1f601.png
  1f606.png
  1f605.png
  1f602.png
  1f609.png
  1f60a.png
  1f60d.png
  1f618.png
  1f60b.png
  1f61b.png
  1f61c.png
  1f60f.png
  1f612.png
  1f62c.png
  1f632.png
  1f633.png
  1f622.png
  1f62d.png
  1f631.png
  1f634.png
  1f637.png
  1f62a.png
  1f62b.png
  1f629.png
  1f611.png
  1f610.png
  1f636.png
  1f614.png
  1f615.png
  1f624.png
  1f44d.png
  1f44e.png
  1f44c.png
  270c.png
  1f44f.png
  1f64c.png
  1f450.png
  1f64f.png
  1f4aa.png
  1f44b.png
  1f448.png
  1f449.png
  1f446.png
  1f447.png
  261d.png
  1f44a.png
  270a.png
  270b.png
  1f476.png
  1f466.png
  1f467.png
  1f468.png
  1f469.png
  1f474.png
  1f475.png
  1f46e.png
  1f575.png
  1f481.png
  1f64b.png
  1f646.png
  1f645.png
  1f647.png
  1f483.png
  1f6b6.png
  1f3c3.png
  1f3ca.png
  1f6b4.png
  1f46a.png
  1f46b.png
  1f6c0.png
  1f436.png
  1f431.png
  1f42d.png
  1f43b.png
  1f43c.png
  1f428.png
  1f42f.png
  1f42e.png
  1f437.png
  1f438.png
  1f435.png
  1f414.png
  1f427.png
  1f41d.png
  1f41f.png
  1f419.png
  1f433.png
  1f42c.png
  1f34e.png
  1f34c.png
  1f347.png
  1f349.png
  1f353.png
  1f351.png
  1f35e.png
  1f35a.png
  1f35c.png
  1f355.png
  1f354.png
  1f35f.png
  1f370.png
  2615.png
  1f37a.png
  1f377.png
  1f434.png
  1f439.png
  1f430.png
  1f33d.png
  1f345.png
  1f36a.png
  1f4bb.png
  1f5a5.png
  2328.png
  1f5b1.png
  1f4f1.png
  1f4de.png
  1f4f7.png
  1f50b.png
  1f4a1.png
  1f50d.png
  1f512.png
  1f511.png
  1f527.png
  1f528.png
  1f4cc.png
  1f4ce.png
  2702.png
  1f4dd.png
  1f4d3.png
  1f4da.png
  1f4d6.png
  270f.png
  1f4b0.png
  1f4b5.png
  1f4b3.png
  23f0.png
  231a.png
  1f381.png
  1f388.png
  1f389.png
  1f382.png
  1f3c6.png
  1f3b8.png
  1f3ae.png
  1f3b2.png
  1f514.png
  1f4e2.png
  1f3a7.png
  1f3a4.png
  2764.png
  1f49b.png
  1f49a.png
  1f499.png
  1f49c.png
  1f494.png
  1f495.png
  1f4af.png
  2705.png
  274c.png
  26a0.png
  2757.png
  2753.png
  2b50.png
  1f31f.png
  2728.png
  1f525.png
  1f4a5.png
  1f4ab.png
  26a1.png
  2600.png
  1f308.png
  1f319.png
  2744.png
  1f4a4.png
  1f4a2.png
  1f4ac.png
  1f4ad.png
  267b.png
  2795.png
  2796.png
  1f6ab.png
  26d4.png
  1f195.png
  1f197.png
  1f192.png
  1f51d.png
  1f534.png
  1f490.png
  1f339.png
  1f680.png
  2708.png
  1f689.png
  1f697.png
  1f695.png
  1f68c.png
  1f691.png
  1f692.png
  1f693.png
  1f6b2.png
  1f6a2.png
  26f5.png
  1f682.png
  1f5fa.png
  1f5fd.png
  1f5fc.png
  1f3f0.png
  1f3a1.png
  1f3a2.png
  26f1.png
  1f30b.png
  1f5fb.png
  1f30a.png
  1f30d.png
  1f30f.png
  1f30e.png
  1f3e0.png
  1f3e2.png
  1f3e5.png
  1f3e6.png
  1f3eb.png
  26e9.png
  1f3ef.png
  1f303.png
  1f305.png
  1f304.png
  1f699.png
  1f69a.png
  26fd.png
  26ea.png
  1f1e8-1f1f3.png
  1f1fa-1f1f8.png
  1f1ec-1f1e7.png
  1f1ef-1f1f5.png
  1f1f0-1f1f7.png
  1f1eb-1f1f7.png
  1f1e9-1f1ea.png
  1f1ee-1f1f9.png
  1f1ea-1f1f8.png
  1f1f5-1f1f9.png
  1f1f7-1f1fa.png
  1f1ee-1f1f3.png
  1f1e7-1f1f7.png
  1f1e8-1f1e6.png
  1f1e6-1f1fa.png
  1f1f3-1f1ff.png
  1f1f8-1f1ec.png
  1f1f2-1f1fe.png
  1f1f9-1f1ed.png
  1f1fb-1f1f3.png
  1f1f5-1f1ed.png
  1f1ee-1f1e9.png
  1f1f3-1f1f1.png
  1f1e8-1f1ed.png
  1f1f8-1f1ea.png
  1f1f3-1f1f4.png
  1f1eb-1f1ee.png
  1f1e9-1f1f0.png
  1f1f5-1f1f1.png
  1f1e7-1f1ea.png
  1f1ec-1f1f7.png
  1f1f9-1f1f7.png
  1f1ea-1f1ec.png
  1f1ff-1f1e6.png
  1f1e6-1f1f7.png
  1f1e8-1f1f1.png
  1f1f2-1f1fd.png
  1f1f8-1f1e6.png
  1f1e6-1f1ea.png
  1f1fa-1f1e6.png
)

fail=0

if [[ "${1:-}" != "--verify" ]]; then
  echo "下载 Twemoji ${TAG} assets/72x72 共 ${#NAMES[@]} 枚 -> ${DEST}"
  # 单枚 curl 失败不中止(否则走不到下面的缺枚清点),缺枚由校验段如实列出
  printf '%s\n' "${NAMES[@]}" | xargs -P 8 -I{} curl -sf -o "${DEST}/{}" "${BASE}/{}" || true
fi

# 自校验:逐一存在、非空、PNG 魔数、72x72(IHDR 宽高均 0x48);缺枚如实列出。
for name in "${NAMES[@]}"; do
  file="${DEST}/${name}"
  if [[ ! -s "$file" ]]; then
    echo "缺失或空文件: ${name}" >&2
    fail=1
    continue
  fi
  magic="$(head -c 24 "$file" | od -An -tx1 | tr -d ' \n')"
  if [[ "$magic" != 89504e470d0a1a0a* ]]; then
    echo "非 PNG: ${name}" >&2
    fail=1
  elif [[ "$magic" != *494844520000004800000048* ]]; then
    echo "非 72x72(IHDR 宽高 != 0x48): ${name}" >&2
    fail=1
  fi
done
total="$(du -cb "${NAMES[@]/#/$DEST/}" | tail -1 | cut -f1)"
echo "核对: ${#NAMES[@]} 枚, 总体积 ${total} B"
if (( fail )); then
  echo "校验未过:存在缺枚/坏文件(缺枚单元运行时回落黑白字形,不阻塞构建)" >&2
  exit 1
fi
echo "全部 ${#NAMES[@]} 枚在位且为合法 72x72 PNG。"
