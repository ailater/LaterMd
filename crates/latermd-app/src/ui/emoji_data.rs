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
}
