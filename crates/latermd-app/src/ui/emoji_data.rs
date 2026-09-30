//! Emoji 面板的内置数据表(docs/emoji-plan.md §4「甲 · 内置精简表」,
//! decisions-pending #36 默认口径:零新增依赖,手写常量)。
//!
//! ## 选录规则(emoji-plan §4/§7)
//!
//! - **只收默认肤色**:不含肤色修饰符(U+1F3FB..U+1F3FF),不做变体展开。
//! - **ZWJ 序列与单组件二选一**:同一概念要么收序列、要么收组件,不重复
//!   占位(本表全收单组件,ZWJ 序列的逐组件退格行为见 emoji-plan §7 #3)。
//! - **每类 ≤ 40 枚**:限制单屏渲进字体图集的字形量(图集是有限纹理,
//!   emoji-plan §4 推荐甲的理由 2)。
//! - 中文名 / 英文名 / 短码(gemoji 习用名,不含冒号)三字段都为 E2 的
//!   三路搜索与 E4 的 `:shortcode:` 补全预留。
//!
//! 应用内由 NotoEmoji(egui 出厂字体链,fonts.rs 保序)黑白渲染;导出
//! HTML / 粘贴到外部仍是系统彩色字体 —— 这是上游事实,不是 bug。
//!
//! ## E3 的口径:数据保留,渲染过滤
//!
//! 本模块只提供「字形可用集合」(`GlyphSet`)、出厂字体的 cmap format 12
//! 核验(`from_font_cmap12`)与**注入谓词**的过滤函数(`visible_entries` /
//! `visible_hits`);核验的组装与缓存在 `emoji_panel`(UI 侧)首帧做一次。
//!
//! **为什么不用 `Fonts::has_glyphs` 探测**(emoji-plan F4 / E3 原方案):
//! epaint 0.36.2 的替换字形 ◻(U+25FB)恰好就在 NotoEmoji-Regular 里,
//! `has_glyph` 判「字形脸 == 替换脸 → 没字形」,凡该脸拥有的字符一律
//! 误报 false —— 实测全表 0/272 枚通过(decisions-pending #52,本棒在
//! E3 复测同结果)。渲染 shaping 走 harfrust 独立解析,不受其害;故改
//! 按 #52 的 cmap 直验口径运行时化,数据表的入库清洗(#52)由此获得
//! 一道常驻回归防线。

use std::collections::HashSet;

/// 单枚条目:字符 + 中英双语名 + 短码。
#[derive(Debug, Clone, Copy)]
pub struct EmojiEntry {
    /// Unicode 字符。单组件(或旗帜的双码位),只收默认肤色。
    pub char: &'static str,
    /// 中文名(E2 搜索用)。
    pub name_zh: &'static str,
    /// 英文名。
    pub name_en: &'static str,
    /// 短码(gemoji 习用名,不含冒号)。
    pub shortcode: &'static str,
}

/// 一个分类:标签名 + 条目表。
#[derive(Debug, Clone, Copy)]
pub struct EmojiGroup {
    /// 分类标签(横向标签页标题)。
    pub name: &'static str,
    /// 条目(每类 ≤ 40,见模块文档)。
    pub entries: &'static [EmojiEntry],
}

/// 表格行的紧凑写法:每行一条 `(字符, 中文名, 英文名, 短码)`。
///
/// 定义须在 `GROUPS` 之前:`macro_rules!` 按文本顺序生效。
macro_rules! entries {
    ($(($c:literal, $zh:literal, $en:literal, $sc:literal)),* $(,)?) => {
        &[$(EmojiEntry { char: $c, name_zh: $zh, name_en: $en, shortcode: $sc }),*]
    };
}

/// 三路大小写不敏感匹配(docs/emoji-plan.md E2):查询对**中文名 / 英文
/// 名 / 短码**做子串包含;`to_lowercase` 对 CJK 是恒等变换,只归一 ASCII
/// 大小写。空查询恒命中(调用方据此走「当前分类全表」分支,不走这里)。
pub fn matches(entry: &EmojiEntry, query: &str) -> bool {
    let needle = query.to_lowercase();
    needle.is_empty()
        || entry.name_zh.to_lowercase().contains(&needle)
        || entry.name_en.to_lowercase().contains(&needle)
        || entry.shortcode.to_lowercase().contains(&needle)
}

/// 跨分类搜索:返回 `(分类下标, 条目)`,按分类顺序、分类内按表序排列
/// (面板据此分段渲染,段头即命中来源)。空查询返回空 —— 空态显示的是
/// 当前分类全表,不是全库。
pub fn search(query: &str) -> Vec<(usize, &'static EmojiEntry)> {
    let needle = query.trim().to_lowercase();
    let mut hits = Vec::new();
    if needle.is_empty() {
        return hits;
    }
    for (group_index, group) in GROUPS.iter().enumerate() {
        for entry in group.entries {
            if matches(entry, &needle) {
                hits.push((group_index, entry));
            }
        }
    }
    hits
}

/// 字形可用集合(E3):探测通过的字符白名单,面板据此把缺字形条目从
/// 渲染中剔除,数据表本身不动。空集 = 全部缺字形(面板各分类显示占位),
/// 与面板状态里的「未探测」(`None`)是两回事。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlyphSet(HashSet<String>);

impl GlyphSet {
    /// 收录一枚探测通过的字符(可能多码位:旗帜、带 FE0F 的条目)。
    pub fn insert(&mut self, glyph: &str) {
        self.0.insert(glyph.to_owned());
    }

    /// 该字符是否探测通过。
    pub fn allows(&self, glyph: &str) -> bool {
        self.0.contains(glyph)
    }

    /// 是否空集(核验失败/全缺的信号,调用方据此走兜底)。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// 全量集:所有条目视为有字形。核验兜底(出厂字体键名缺失/解析失败
    /// 时不报废面板)与无头测试注入用 —— 让「过滤行为」与「真实字形」
    /// 解耦(emoji-plan §7 #7)。
    pub fn all() -> Self {
        Self(
            GROUPS
                .iter()
                .flat_map(|g| g.entries.iter())
                .map(|e| e.char.to_owned())
                .collect(),
        )
    }

    /// 按出厂字体的 cmap format 12 覆盖核验数据表(decisions-pending #52
    /// 口径),收录每个实际需要字形的码位都被覆盖的条目。FE0F(VS16)与
    /// U+200D(ZWJ)是 default-ignorable,shaping 时剥离、不产生字形
    /// 需求,核验前剥掉。字体解析失败返回空集(调用方兜底)。
    pub fn from_font_cmap12(font: &[u8]) -> Self {
        let mut set = Self::default();
        if let Some(cmap) = Cmap12::parse(font) {
            for entry in GROUPS.iter().flat_map(|g| g.entries.iter()) {
                let covered = entry
                    .char
                    .chars()
                    .filter(|&c| c != '\u{FE0F}' && c != '\u{200D}')
                    .all(|c| cmap.covers(c as u32));
                if covered {
                    set.insert(entry.char);
                }
            }
        }
        set
    }

    /// 已收录字符(测试契约用:核验输出必须是数据表的子集)。
    #[cfg(test)]
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }
}

/// 大端 u16 / u32 读取,越界返回 `None`(不 panic,坏字体走兜底)。
fn be_u16(data: &[u8], offset: usize) -> Option<u16> {
    let bytes = data.get(offset..offset + 2)?;
    Some(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn be_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// 解析后的 sfnt cmap format 12 子表视图。只认 fmt12:运行时对 fmt4-only
/// 的码位同样取不到字形,按「两张子表并集」口径会漏报豆腐块(#52)。
struct Cmap12<'a> {
    data: &'a [u8],
    /// groups 数组的起始偏移(format 12 头 16 字节之后)。
    groups: usize,
    /// group 数量。
    count: usize,
}

impl<'a> Cmap12<'a> {
    /// 从 sfnt 容器解析:定位 cmap 表,再在 encoding records 里选
    /// format 12 子表((3,10) Windows UCS-4 优先,(0,4..=6) Unicode 全库兜底)。
    fn parse(data: &'a [u8]) -> Option<Self> {
        let num_tables = be_u16(data, 4)? as usize;
        let mut cmap = None;
        for i in 0..num_tables {
            let record = 12 + i * 16;
            if record + 16 > data.len() {
                return None;
            }
            if &data[record..record + 4] == b"cmap" {
                cmap = Some(be_u32(data, record + 8)? as usize);
                break;
            }
        }
        let cmap = cmap?;
        let encodings = be_u16(data, cmap + 2)? as usize;
        let mut best: Option<(usize, usize)> = None; // (优先级, 子表偏移)
        for i in 0..encodings {
            let record = cmap + 4 + i * 8;
            if record + 8 > data.len() {
                return None;
            }
            let rank = match (be_u16(data, record)?, be_u16(data, record + 2)?) {
                (3, 10) => 2,
                (0, 4..=6) => 1,
                _ => 0,
            };
            if rank == 0 {
                continue;
            }
            let sub = cmap + be_u32(data, record + 4)? as usize;
            if sub + 4 > data.len() || be_u16(data, sub)? != 12 {
                continue;
            }
            if best.is_none_or(|(r, _)| rank > r) {
                best = Some((rank, sub));
            }
        }
        let (_, sub) = best?;
        // format 12 头:format/reserved(u16×2)+ length/language(u32×2)
        // + nGroups(u32),共 16 字节;groups 每条 12 字节
        let count = be_u32(data, sub + 12)? as usize;
        let groups = sub + 16;
        if groups + count * 12 > data.len() {
            return None;
        }
        Some(Self {
            data,
            groups,
            count,
        })
    }

    /// 二分查码位是否落在某 group 的 [start, end] 区间内。
    fn covers(&self, cp: u32) -> bool {
        let mut lo = 0;
        let mut hi = self.count;
        while lo < hi {
            let mid = (lo + hi) / 2;
            let record = self.groups + mid * 12;
            let (Some(start), Some(end)) =
                (be_u32(self.data, record), be_u32(self.data, record + 4))
            else {
                return false;
            };
            if cp < start {
                hi = mid;
            } else if cp > end {
                lo = mid + 1;
            } else {
                return true;
            }
        }
        false
    }
}

/// E3 过滤:分类表剔除缺字形条目,保表序。`has` 注入 —— 生产侧是探测
/// 缓存的 [`GlyphSet::allows`],单测给假谓词(不碰真实字体)。
pub fn visible_entries(entries: &[EmojiEntry], has: impl Fn(&str) -> bool) -> Vec<&EmojiEntry> {
    entries.iter().filter(|entry| has(entry.char)).collect()
}

/// E3 过滤:搜索命中按同一谓词剔除,保 (组号, 表序) —— 面板的来源
/// 分段依赖该顺序。
pub fn visible_hits(
    hits: Vec<(usize, &'static EmojiEntry)>,
    has: impl Fn(&str) -> bool,
) -> Vec<(usize, &'static EmojiEntry)> {
    hits.into_iter()
        .filter(|(_, entry)| has(entry.char))
        .collect()
}

/// 八个分类,顺序即面板标签页顺序。
pub const GROUPS: [EmojiGroup; 8] = [
    EmojiGroup {
        name: "表情",
        entries: entries![
            ("😀", "笑脸", "grinning", "grinning"),
            ("😃", "开怀笑", "smiley", "smiley"),
            ("😄", "开心笑", "smile", "smile"),
            ("😁", "露齿笑", "beaming", "grin"),
            ("😆", "大笑", "laughing", "laughing"),
            ("😅", "冒汗笑", "sweat smile", "sweat_smile"),
            ("😂", "笑哭", "joy", "joy"),
            ("😉", "眨眼", "winking", "wink"),
            ("😊", "害羞笑", "smiling blush", "blush"),
            ("😍", "爱慕", "heart eyes", "heart_eyes"),
            ("😘", "飞吻", "kiss heart", "kissing_heart"),
            ("😋", "好吃", "yum", "yum"),
            ("😛", "吐舌", "tongue out", "stuck_out_tongue"),
            (
                "😜",
                "眨眼吐舌",
                "winking tongue",
                "stuck_out_tongue_winking_eye"
            ),
            ("😏", "得意", "smirking", "smirk"),
            ("😒", "不悦", "unamused", "unamused"),
            ("😬", "呲牙", "grimacing", "grimacing"),
            ("😲", "震惊", "astonished", "astonished"),
            ("😳", "脸红", "flushed", "flushed"),
            ("😢", "哭", "crying", "cry"),
            ("😭", "大哭", "sobbing", "sob"),
            ("😱", "尖叫", "screaming", "scream"),
            ("😴", "睡觉", "sleeping", "sleeping"),
            ("😷", "口罩", "mask", "mask"),
            ("😪", "瞌睡", "sleepy", "sleepy"),
            ("😫", "累", "tired", "tired_face"),
            ("😩", "叹气", "weary", "weary"),
            ("😑", "无语", "expressionless", "expressionless"),
            ("😐", "面无表情", "neutral", "neutral_face"),
            ("😶", "沉默", "no mouth", "no_mouth"),
            ("😔", "低落", "pensive", "pensive"),
            ("😕", "困惑", "confused", "confused"),
            ("😤", "鼓劲", "triumph", "triumph"),
        ],
    },
    EmojiGroup {
        name: "手势",
        entries: entries![
            ("👍", "点赞", "thumbs up", "+1"),
            ("👎", "不赞", "thumbs down", "-1"),
            ("👌", "OK", "OK hand", "ok_hand"),
            ("✌️", "耶", "victory", "v"),
            ("👏", "鼓掌", "clapping", "clap"),
            ("🙌", "举双手", "raising hands", "raised_hands"),
            ("👐", "张开双手", "open hands", "open_hands"),
            ("🙏", "感谢", "folded hands", "pray"),
            ("💪", "加油", "flexed biceps", "muscle"),
            ("👋", "招手", "waving hand", "wave"),
            ("👈", "指左", "point left", "point_left"),
            ("👉", "指右", "point right", "point_right"),
            ("👆", "指上", "point up", "point_up_2"),
            ("👇", "指下", "point down", "point_down"),
            ("☝️", "食指向上", "index up", "point_up"),
            ("👊", "出拳", "punching", "punch"),
            ("✊", "举拳", "raised fist", "fist"),
            ("✋", "举手", "raised hand", "raised_hand"),
        ],
    },
    EmojiGroup {
        name: "人物",
        entries: entries![
            ("👶", "婴儿", "baby", "baby"),
            ("👦", "男孩", "boy", "boy"),
            ("👧", "女孩", "girl", "girl"),
            ("👨", "男人", "man", "man"),
            ("👩", "女人", "woman", "woman"),
            ("👴", "老爷爷", "old man", "older_man"),
            ("👵", "老奶奶", "old woman", "older_woman"),
            ("👮", "警察", "police officer", "cop"),
            ("🕵️", "侦探", "detective", "detective"),
            ("💁", "提示", "tipping hand", "information_desk_person"),
            ("🙋", "举手", "raising hand", "raising_hand"),
            ("🙆", "OK 手势", "gesturing OK", "ok_woman"),
            ("🙅", "拒绝", "gesturing no", "no_good"),
            ("🙇", "鞠躬", "bowing", "bow"),
            ("💃", "跳舞的女人", "woman dancing", "dancer"),
            ("🚶", "走路", "walking", "walking"),
            ("🏃", "跑步", "running", "runner"),
            ("🏊", "游泳", "swimming", "swimmer"),
            ("🚴", "骑车", "biking", "bicyclist"),
            ("👪", "家庭", "family", "family"),
            ("👫", "情侣", "couple", "couple"),
            ("🛀", "洗澡", "bath", "bath"),
        ],
    },
    EmojiGroup {
        name: "动物与食物",
        entries: entries![
            ("🐶", "狗", "dog", "dog"),
            ("🐱", "猫", "cat", "cat"),
            ("🐭", "老鼠", "mouse", "mouse"),
            ("🐻", "熊", "bear", "bear"),
            ("🐼", "熊猫", "panda", "panda_face"),
            ("🐨", "考拉", "koala", "koala"),
            ("🐯", "老虎", "tiger", "tiger"),
            ("🐮", "牛", "cow", "cow"),
            ("🐷", "猪", "pig", "pig"),
            ("🐸", "青蛙", "frog", "frog"),
            ("🐵", "猴", "monkey", "monkey_face"),
            ("🐔", "鸡", "chicken", "chicken"),
            ("🐧", "企鹅", "penguin", "penguin"),
            ("🐝", "蜜蜂", "bee", "bee"),
            ("🐟", "鱼", "fish", "fish"),
            ("🐙", "章鱼", "octopus", "octopus"),
            ("🐳", "鲸", "whale", "whale"),
            ("🐬", "海豚", "dolphin", "dolphin"),
            ("🍎", "苹果", "apple", "apple"),
            ("🍌", "香蕉", "banana", "banana"),
            ("🍇", "葡萄", "grapes", "grapes"),
            ("🍉", "西瓜", "watermelon", "watermelon"),
            ("🍓", "草莓", "strawberry", "strawberry"),
            ("🍑", "桃子", "peach", "peach"),
            ("🍞", "面包", "bread", "bread"),
            ("🍚", "米饭", "rice", "rice"),
            ("🍜", "面条", "noodles", "ramen"),
            ("🍕", "披萨", "pizza", "pizza"),
            ("🍔", "汉堡", "hamburger", "hamburger"),
            ("🍟", "薯条", "fries", "fries"),
            ("🍰", "蛋糕", "cake slice", "cake"),
            ("☕", "咖啡", "coffee", "coffee"),
            ("🍺", "啤酒", "beer", "beer"),
            ("🍷", "红酒", "wine", "wine_glass"),
            ("🐴", "马", "horse", "horse"),
            ("🐹", "仓鼠", "hamster", "hamster"),
            ("🐰", "兔子", "rabbit", "rabbit"),
            ("🌽", "玉米", "corn", "corn"),
            ("🍅", "番茄", "tomato", "tomato"),
            ("🍪", "饼干", "cookie", "cookie"),
        ],
    },
    EmojiGroup {
        name: "物品",
        entries: entries![
            ("💻", "笔记本", "laptop", "computer"),
            ("🖥️", "台式机", "desktop", "desktop_computer"),
            ("⌨️", "键盘", "keyboard", "keyboard"),
            ("🖱️", "鼠标", "mouse", "computer_mouse"),
            ("📱", "手机", "phone", "iphone"),
            ("📞", "电话", "telephone", "telephone"),
            ("📷", "相机", "camera", "camera"),
            ("🔋", "电池", "battery", "battery"),
            ("💡", "灯泡", "bulb", "bulb"),
            ("🔍", "放大镜", "magnifier", "mag"),
            ("🔒", "锁", "locked", "lock"),
            ("🔑", "钥匙", "key", "key"),
            ("🔧", "扳手", "wrench", "wrench"),
            ("🔨", "锤子", "hammer", "hammer"),
            ("📌", "图钉", "pushpin", "pushpin"),
            ("📎", "回形针", "paperclip", "paperclip"),
            ("✂️", "剪刀", "scissors", "scissors"),
            ("📝", "备忘", "memo", "memo"),
            ("📓", "笔记本册", "notebook", "notebook"),
            ("📚", "书堆", "books", "books"),
            ("📖", "打开的书", "open book", "open_book"),
            ("✏️", "铅笔", "pencil", "pencil2"),
            ("💰", "钱袋", "money bag", "moneybag"),
            ("💵", "美元", "dollar", "dollar"),
            ("💳", "信用卡", "credit card", "credit_card"),
            ("⏰", "闹钟", "alarm clock", "alarm_clock"),
            ("⌚", "手表", "watch", "watch"),
            ("🎁", "礼物", "gift", "gift"),
            ("🎈", "气球", "balloon", "balloon"),
            ("🎉", "撒花", "party popper", "tada"),
            ("🎂", "生日蛋糕", "birthday cake", "birthday"),
            ("🏆", "奖杯", "trophy", "trophy"),
            ("🎸", "吉他", "guitar", "guitar"),
            ("🎮", "游戏手柄", "game controller", "video_game"),
            ("🎲", "骰子", "dice", "game_die"),
            ("🔔", "铃铛", "bell", "bell"),
            ("📢", "喇叭", "loudspeaker", "loudspeaker"),
            ("🎧", "耳机", "headphone", "headphone"),
            ("🎤", "麦克风", "microphone", "microphone"),
        ],
    },
    EmojiGroup {
        name: "符号",
        entries: entries![
            ("❤️", "红心", "red heart", "heart"),
            ("💛", "黄心", "yellow heart", "yellow_heart"),
            ("💚", "绿心", "green heart", "green_heart"),
            ("💙", "蓝心", "blue heart", "blue_heart"),
            ("💜", "紫心", "purple heart", "purple_heart"),
            ("💔", "心碎", "broken heart", "broken_heart"),
            ("💕", "两颗心", "two hearts", "two_hearts"),
            ("💯", "满分", "hundred", "100"),
            ("✅", "确定", "check mark", "white_check_mark"),
            ("❌", "取消", "cross mark", "x"),
            ("⚠️", "警告", "warning", "warning"),
            ("❗", "感叹", "exclamation", "exclamation"),
            ("❓", "疑问", "question", "question"),
            ("⭐", "星", "star", "star"),
            ("🌟", "闪星", "glowing star", "star2"),
            ("✨", "闪光", "sparkles", "sparkles"),
            ("🔥", "火", "fire", "fire"),
            ("💥", "爆", "collision", "boom"),
            ("💫", "星眩", "dizzy", "dizzy"),
            ("⚡", "闪电", "zap", "zap"),
            ("☀️", "晴", "sun", "sunny"),
            ("🌈", "彩虹", "rainbow", "rainbow"),
            ("🌙", "月亮", "crescent moon", "crescent_moon"),
            ("❄️", "雪花", "snowflake", "snowflake"),
            ("💤", "睡", "zzz", "zzz"),
            ("💢", "生气", "anger", "anger"),
            ("💬", "对话气泡", "speech balloon", "speech_balloon"),
            ("💭", "想法", "thought balloon", "thought_balloon"),
            ("♻️", "回收", "recycle", "recycle"),
            ("➕", "加", "plus", "heavy_plus_sign"),
            ("➖", "减", "minus", "heavy_minus_sign"),
            ("🚫", "禁止", "prohibited", "no_entry"),
            ("⛔", "禁行", "no entry", "no_entry_sign"),
            ("🆕", "新", "new", "new"),
            ("🆗", "OK 键", "OK button", "ok"),
            ("🆒", "酷", "cool", "cool"),
            ("🔝", "置顶", "TOP", "top"),
            ("🔴", "红圆", "red circle", "red_circle"),
            ("💐", "花束", "bouquet", "bouquet"),
            ("🌹", "玫瑰", "rose", "rose"),
        ],
    },
    EmojiGroup {
        name: "旅行",
        entries: entries![
            ("🚀", "火箭", "rocket", "rocket"),
            ("✈️", "飞机", "airplane", "airplane"),
            ("🚉", "车站", "station", "station"),
            ("🚗", "汽车", "car", "car"),
            ("🚕", "出租车", "taxi", "taxi"),
            ("🚌", "公交", "bus", "bus"),
            ("🚑", "救护车", "ambulance", "ambulance"),
            ("🚒", "消防车", "fire engine", "fire_engine"),
            ("🚓", "警车", "police car", "police_car"),
            ("🚲", "自行车", "bicycle", "bike"),
            ("🚢", "轮船", "ship", "ship"),
            ("⛵", "帆船", "sailboat", "boat"),
            ("🚂", "火车", "locomotive", "steam_locomotive"),
            ("🗺️", "地图", "world map", "world_map"),
            ("🗽", "自由女神", "Statue of Liberty", "statue_of_liberty"),
            ("🗼", "东京塔", "Tokyo Tower", "tokyo_tower"),
            ("🏰", "城堡", "castle", "european_castle"),
            ("🎡", "摩天轮", "ferris wheel", "ferris_wheel"),
            ("🎢", "过山车", "roller coaster", "roller_coaster"),
            ("⛱️", "沙滩", "beach umbrella", "beach_umbrella"),
            ("🌋", "火山", "volcano", "volcano"),
            ("🗻", "富士山", "Mount Fuji", "mount_fuji"),
            ("🌊", "海浪", "wave", "ocean"),
            ("🌍", "地球欧非", "globe Europe-Africa", "earth_africa"),
            ("🌏", "地球亚澳", "globe Asia-Australia", "earth_asia"),
            ("🌎", "地球美洲", "globe Americas", "earth_americas"),
            ("🏠", "房子", "house", "house"),
            ("🏢", "写字楼", "office building", "office"),
            ("🏥", "医院", "hospital", "hospital"),
            ("🏦", "银行", "bank", "bank"),
            ("🏫", "学校", "school", "school"),
            ("⛩️", "鸟居", "shinto shrine", "shinto_shrine"),
            ("🏯", "天守阁", "Japanese castle", "japanese_castle"),
            ("🌃", "星夜", "night", "night_with_stars"),
            ("🌅", "黎明", "sunrise", "sunrise"),
            (
                "🌄",
                "山间日出",
                "sunrise over mountains",
                "sunrise_over_mountains"
            ),
            ("🚙", "越野车", "SUV", "suv"),
            ("🚚", "货车", "truck", "truck"),
            ("⛽", "加油", "fuel pump", "fuel"),
            ("⛪", "教堂", "church", "church"),
        ],
    },
    EmojiGroup {
        name: "旗帜",
        entries: entries![
            ("🇨🇳", "中国", "China", "cn"),
            ("🇺🇸", "美国", "United States", "us"),
            ("🇬🇧", "英国", "United Kingdom", "gb"),
            ("🇯🇵", "日本", "Japan", "jp"),
            ("🇰🇷", "韩国", "South Korea", "kr"),
            ("🇫🇷", "法国", "France", "fr"),
            ("🇩🇪", "德国", "Germany", "de"),
            ("🇮🇹", "意大利", "Italy", "it"),
            ("🇪🇸", "西班牙", "Spain", "es"),
            ("🇵🇹", "葡萄牙", "Portugal", "pt"),
            ("🇷🇺", "俄罗斯", "Russia", "ru"),
            ("🇮🇳", "印度", "India", "in"),
            ("🇧🇷", "巴西", "Brazil", "br"),
            ("🇨🇦", "加拿大", "Canada", "ca"),
            ("🇦🇺", "澳大利亚", "Australia", "au"),
            ("🇳🇿", "新西兰", "New Zealand", "nz"),
            ("🇸🇬", "新加坡", "Singapore", "sg"),
            ("🇲🇾", "马来西亚", "Malaysia", "my"),
            ("🇹🇭", "泰国", "Thailand", "th"),
            ("🇻🇳", "越南", "Vietnam", "vn"),
            ("🇵🇭", "菲律宾", "Philippines", "ph"),
            ("🇮🇩", "印度尼西亚", "Indonesia", "id"),
            ("🇳🇱", "荷兰", "Netherlands", "nl"),
            ("🇨🇭", "瑞士", "Switzerland", "ch"),
            ("🇸🇪", "瑞典", "Sweden", "se"),
            ("🇳🇴", "挪威", "Norway", "no"),
            ("🇫🇮", "芬兰", "Finland", "fi"),
            ("🇩🇰", "丹麦", "Denmark", "dk"),
            ("🇵🇱", "波兰", "Poland", "pl"),
            ("🇧🇪", "比利时", "Belgium", "be"),
            ("🇬🇷", "希腊", "Greece", "gr"),
            ("🇹🇷", "土耳其", "Türkiye", "tr"),
            ("🇪🇬", "埃及", "Egypt", "eg"),
            ("🇿🇦", "南非", "South Africa", "za"),
            ("🇦🇷", "阿根廷", "Argentina", "ar"),
            ("🇨🇱", "智利", "Chile", "cl"),
            ("🇲🇽", "墨西哥", "Mexico", "mx"),
            ("🇸🇦", "沙特阿拉伯", "Saudi Arabia", "sa"),
            ("🇦🇪", "阿联酋", "United Arab Emirates", "ae"),
            ("🇺🇦", "乌克兰", "Ukraine", "ua"),
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn all_entries() -> impl Iterator<Item = &'static EmojiEntry> {
        GROUPS.iter().flat_map(|group| group.entries.iter())
    }

    /// 规模口径:每类 ≤ 40(限制单屏字形量),总数 ≥ 250(覆盖日常高频,
    /// emoji-plan §4 甲案的 ~300 目标区间)。
    #[test]
    fn group_sizes_and_total_are_within_bounds() {
        for group in &GROUPS {
            assert!(
                group.entries.len() <= 40,
                "分类「{}」{} 枚,超过 40 上限",
                group.name,
                group.entries.len()
            );
            assert!(!group.entries.is_empty(), "分类「{}」为空", group.name);
        }
        let total: usize = GROUPS.iter().map(|group| group.entries.len()).sum();
        assert!(total >= 250, "总数 {total} 枚,低于 250 下限");
    }

    /// 无重复字符:同一枚 emoji 在全表只出现一次(重复收会让网格与「最近
    /// 使用」出现两个同形条目,且多半意味着两分类边界没切干净)。
    #[test]
    fn no_duplicate_characters() {
        let mut seen = HashSet::new();
        for entry in all_entries() {
            assert!(
                seen.insert(entry.char),
                "{}(:{}) 重复收录",
                entry.name_zh,
                entry.shortcode
            );
        }
    }

    /// 只收默认肤色:任何条目都不得携带肤色修饰符(U+1F3FB..U+1F3FF)——
    /// 变体会让条目数翻倍,emoji-plan §7 #4 已明确不做。
    #[test]
    fn default_skin_tone_only() {
        for entry in all_entries() {
            assert!(
                !entry
                    .char
                    .chars()
                    .any(|c| matches!(c, '\u{1F3FB}'..='\u{1F3FF}')),
                "{} 携带肤色修饰符",
                entry.name_zh
            );
        }
    }

    /// ZWJ 序列与单组件二选一:某序列(含 U+200D)被收录时,它的组件就
    /// 不得再单独立目,反之亦然 —— 重复占位既浪费网格位,也让「同一个
    /// 表情」出现两个入口(emoji-plan §7 #3)。本表当前全收单组件,断言
    /// 为将来补 ZWJ 条目守住边界。
    #[test]
    fn zwj_sequences_do_not_repeat_their_components() {
        let singles: HashSet<&str> = all_entries().map(|entry| entry.char).collect();
        // 只查真正的 ZWJ 序列:单组件条目按 U+200D 切分得到的「组件」就是
        // 它自己,不适用本断言
        for entry in all_entries().filter(|entry| entry.char.contains('\u{200D}')) {
            for component in entry.char.split('\u{200D}') {
                let stripped: String = component
                    .chars()
                    // 序列组件常带 FE0F 表情形态修饰,单独比较时剥掉
                    .filter(|&c| c != '\u{FE0F}')
                    .collect();
                if stripped.is_empty() {
                    continue;
                }
                assert!(
                    !singles.contains(stripped.as_str()),
                    "{}(序列)与单组件 {} 重复收录",
                    entry.name_zh,
                    stripped
                );
            }
        }
    }

    /// 三个文本字段都非空:中文名 / 英文名 / 短码是 E2 搜索与 E4 补全的
    /// 数据面,缺一个就是半个条目。
    #[test]
    fn every_entry_has_all_three_names() {
        for entry in all_entries() {
            assert!(!entry.name_zh.is_empty(), "{:?} 缺中文名", entry.char);
            assert!(!entry.name_en.is_empty(), "{:?} 缺英文名", entry.char);
            assert!(!entry.shortcode.is_empty(), "{:?} 缺短码", entry.char);
        }
    }

    /// E2 三路匹配之一:中文名子串(火箭 → 🚀)。
    #[test]
    fn search_matches_chinese_name() {
        let hits = search("火箭");
        assert_eq!(hits.len(), 1, "「火箭」只命中一枚:{hits:?}");
        assert_eq!(hits[0].1.char, "🚀");
        assert!(matches(hits[0].1, "火箭"));
    }

    /// E2 三路匹配之二:英文名大小写不敏感(ROCK → rocket → 🚀)。
    #[test]
    fn search_matches_english_name_case_insensitively() {
        let hits = search("ROCK");
        assert!(hits.iter().any(|(_, entry)| entry.char == "🚀"), "{hits:?}");
        assert!(hits
            .iter()
            .all(|(_, entry)| entry.name_en.to_lowercase().contains("rock")));
    }

    /// E2 三路匹配之三:短码(tada 不出现在任何中英文名里,只命中 🎉)。
    #[test]
    fn search_matches_shortcode() {
        let hits = search("tada");
        assert_eq!(hits.len(), 1, "「tada」只命中一枚:{hits:?}");
        assert_eq!(hits[0].1.char, "🎉");
    }

    /// 空查询不展开成全库(空态走「当前分类全表」,不是跨分类);查不到
    /// 返回空,面板据此显示「无匹配」。
    #[test]
    fn empty_query_and_misses_return_nothing() {
        assert!(search("").is_empty());
        assert!(search("   ").is_empty(), "纯空白等价空查询");
        assert!(search("不存在的词zzz").is_empty());
    }

    /// 命中跨分类:查询「手」至少落在两个分类(手势 / 人物),且按分类
    /// 顺序排列 —— 面板的来源分段依赖这一顺序。
    #[test]
    fn hits_span_groups_in_group_order() {
        let hits = search("手");
        let groups: Vec<usize> = hits.iter().map(|(group, _)| *group).collect();
        assert!(groups.contains(&1), "手势分类有命中:{groups:?}");
        assert!(groups.contains(&2), "人物分类有命中:{groups:?}");
        assert!(
            groups.windows(2).all(|pair| pair[0] <= pair[1]),
            "命中按分类顺序排列:{groups:?}"
        );
    }

    /// E3 过滤(注入谓词,emoji-plan §7 #7):全真 = 等价原表(保序);
    /// 只放行一枚 = 只留它;全假 = 空(面板据此显示「本机字体缺字形」
    /// 占位而非空网格)。谓词一律假闭包,不碰真实字体。
    #[test]
    fn visible_entries_filters_by_injected_predicate() {
        let entries = GROUPS[1].entries; // 手势,18 枚
        let all = visible_entries(entries, |_| true);
        assert_eq!(all.len(), entries.len());
        assert!(
            all.iter()
                .map(|e| e.char)
                .eq(entries.iter().map(|e| e.char)),
            "全真谓词保表序"
        );

        let only_thumbs_up = visible_entries(entries, |g| g == "👍");
        assert_eq!(only_thumbs_up.len(), 1);
        assert_eq!(only_thumbs_up[0].char, "👍");

        assert!(
            visible_entries(entries, |_| false).is_empty(),
            "全假谓词剔光"
        );
    }

    /// E3 过滤(搜索命中):同一谓词作用于 `search` 结果,保 (组号, 表序)
    /// —— 面板的来源分段依赖该顺序不因过滤而乱。
    #[test]
    fn visible_hits_filters_by_injected_predicate() {
        let hits = search("手");
        assert!(hits.len() >= 2, "前置:跨分类命中才谈得上保序");
        let allowed: HashSet<&str> = hits.iter().map(|(_, e)| e.char).take(3).collect();
        let kept = visible_hits(hits, |g| allowed.contains(g));
        assert_eq!(kept.len(), 3);
        assert!(
            kept.windows(2).all(|pair| pair[0].0 <= pair[1].0),
            "过滤后仍按分类顺序:{kept:?}"
        );
        assert!(kept.iter().all(|(_, e)| allowed.contains(e.char)));
    }

    /// E3:`GlyphSet` 的收录 / 查询 / 全量语义。`all()` 是测试注入的
    /// 全量集,必须覆盖数据表每一枚。
    #[test]
    fn glyph_set_allows_inserted_and_all_covers_table() {
        let mut set = GlyphSet::default();
        assert!(!set.allows("🚀"), "空集不放行任何字符");
        set.insert("🚀");
        assert!(set.allows("🚀"));
        assert!(!set.allows("🎉"));

        let all = GlyphSet::all();
        for entry in all_entries() {
            assert!(
                all.allows(entry.char),
                "{}(:{}) 不在全量集",
                entry.name_zh,
                entry.shortcode
            );
        }
        assert_eq!(
            all.iter().count(),
            all_entries().count(),
            "全量集与数据表一一对应"
        );
    }

    /// 构造最小 sfnt:单 cmap 表,单 (3,10) encoding,format 12 子表,
    /// groups 即传入区间。供解析器测试当对照字体。
    fn build_min_font(groups: &[(u32, u32)]) -> Vec<u8> {
        let mut data = Vec::new();
        let put32 = |data: &mut Vec<u8>, v: u32| data.extend_from_slice(&v.to_be_bytes());
        let put16 = |data: &mut Vec<u8>, v: u16| data.extend_from_slice(&v.to_be_bytes());
        put32(&mut data, 0x0001_0000); // sfntVersion
        put16(&mut data, 1); // numTables
        data.extend_from_slice(&[0; 6]); // searchRange / entrySelector / rangeShift
                                         // 表目录记录:tag + checksum + offset + length
        data.extend_from_slice(b"cmap");
        put32(&mut data, 0); // checksum
        put32(&mut data, 28); // cmap 偏移
        let cmap_len = 4 + 8 + 16 + groups.len() * 12;
        put32(&mut data, cmap_len as u32);
        assert_eq!(data.len(), 28);
        // cmap 头 + 单条 encoding record (3,10)
        put16(&mut data, 0); // version
        put16(&mut data, 1); // numTables
        put16(&mut data, 3); // platformID: Windows
        put16(&mut data, 10); // encodingID: UCS-4
        put32(&mut data, 12); // 子表相对 cmap 的偏移
        assert_eq!(data.len(), 40);
        // format 12 子表
        put16(&mut data, 12); // format
        put16(&mut data, 0); // reserved
        put32(&mut data, (16 + groups.len() * 12) as u32); // length
        put32(&mut data, 0); // language
        put32(&mut data, groups.len() as u32); // nGroups
        for &(start, end) in groups {
            put32(&mut data, start);
            put32(&mut data, end);
            put32(&mut data, 0); // startGlyphID,核验用不到
        }
        data
    }

    /// E3 解析器:cmap fmt12 的命中 / 未命中 / 区间边界 / 二分多组。
    #[test]
    fn cmap12_parse_covers_groups() {
        let font = build_min_font(&[(0x1F600, 0x1F601), (0x1F680, 0x1F680)]);
        let set = GlyphSet::from_font_cmap12(&font);
        assert!(set.allows("\u{1F600}"), "区间起点命中");
        assert!(set.allows("\u{1F601}"), "区间终点命中");
        assert!(!set.allows("\u{1F602}"), "区间之外不命中");
        assert!(!set.allows("\u{1F67F}"), "下一区间之前不命中");
        assert!(set.allows("\u{1F680}"), "单码位区间命中");
        assert!(!set.allows("A"), "拉丁不在表内");
    }

    /// E3 解析器:多码位条目要求**每个**实际需要字形的码位都覆盖;
    /// FE0F(VS16)与 ZWJ 是 default-ignorable(shaping 剥离),核验前
    /// 剥掉 —— 不因修饰符把条目误剔。
    #[test]
    fn cmap12_requires_every_codepoint_but_strips_ignorables() {
        let font = build_min_font(&[(0x270C, 0x270C)]); // ✌ 裸码位
        let set = GlyphSet::from_font_cmap12(&font);
        assert!(set.allows("✌️"), "裸码位 + FE0F:修饰符被剥离,不算缺字形");
        assert!(!set.allows("😀"), "其它码位不命中");
    }

    /// E3 解析器:坏字节一律 `None` / 空集(不 panic),生产侧据此走
    /// 全量放行兜底;fmt4-only 的字体不认(运行时同样解析不到,认了
    /// 会漏报豆腐块,#52 口径)。
    #[test]
    fn cmap12_rejects_bad_bytes_and_fmt4_only() {
        let good = build_min_font(&[(0x1F600, 0x1F600)]);
        for cut in [0, 4, 12, 28, 40, 56, good.len() - 1] {
            assert!(
                GlyphSet::from_font_cmap12(&good[..cut]).is_empty(),
                "截断到 {cut} 字节应得空集"
            );
        }
        // fmt4-only:encoding record 指向 format 4 子表 → 解析失败
        let mut fmt4 = build_min_font(&[]);
        let sub = 40usize; // format 12 子表的位置
        fmt4[sub..sub + 2].copy_from_slice(&4u16.to_be_bytes());
        assert!(
            GlyphSet::from_font_cmap12(&fmt4).is_empty(),
            "fmt4-only 不认"
        );
    }
}
