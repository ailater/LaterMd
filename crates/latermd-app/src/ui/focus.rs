//! 专注模式(#64 M2):Live 模式淡化非活动块的决策面。
//!
//! 与打字机(#64 M1)同一分层:纯判定与常量在这里(不 import egui),
//! 落地在 `live` 的富渲染分支 —— 每个淡化块的矩形上叠一层「编辑框背景
//! 色 × [`DIM_ALPHA`]」的填充矩形(#81 内联标记半隐藏同款纯绘制先例:
//! 不注册交互、不参与命中测试),淡化块仍可点击进入编辑
//! (`clicked_for_edit` 读原始指针事件,与遮罩无关),进入即变活动块、
//! 下帧起不再淡化。
//!
//! 源码模式不接线:源码是单个 TextEdit,分段淡化没有纯绘制层的落点
//! (块级遮罩会盖住光标与选区),边界与将来路径登记
//! decisions-pending #122。

/// 淡化判定:活动块与其紧邻块(前后各一,「光标邻块」)保持全对比度,
/// 其余非活动块淡化。没有活动块(光标未落)时全部正常 —— 专注的对象
/// 是「正在编辑的块」,没有编辑焦点就没有淡化(全淡化等于把整篇涂灰,
/// 只剩干扰)。
pub fn dimmed(active: Option<usize>, index: usize) -> bool {
    match active {
        None => false,
        Some(active) => active.abs_diff(index) > 1,
    }
}

/// 非活动块遮罩的不透明度(遮罩色 = 编辑框背景色,与 #81 遮罩同源)。
///
/// 取 110 = 出厂明暗两主题下淡化后正文对比度都保持在 WCAG AA 正文线
/// 4.5:1 之上(按 sRGB 逐通道混合 × WCAG 相对亮度折算:亮主题背景 255/
/// 文字黑 → 混后灰 110 → 5.10:1;暗主题背景 gray(10)/文字 ≈gray(210)
/// → 混后灰 ≈124 → 4.74:1),「明显弱化但不残废」;不做用户可调的
/// 透明度配置面(规格口径,与 #81 同)。
pub const DIM_ALPHA: u8 = 110;

#[cfg(test)]
mod tests {
    use super::*;

    /// 淡化范围表:活动块 ±1(光标邻块)正常,距离 ≥2 淡化;无活动块
    /// 全部正常(取舍登记 decisions-pending #122)。
    #[test]
    fn dimming_table() {
        let active = Some(2);
        let want = [
            (0, true, "隔一块,淡化"),
            (1, false, "紧邻上块,正常"),
            (2, false, "活动块,正常"),
            (3, false, "紧邻下块,正常"),
            (4, true, "隔一块,淡化"),
        ];
        for (index, dim, why) in want {
            assert_eq!(dimmed(active, index), dim, "active=2, index={index}: {why}");
        }
        // 边界:活动块在文档头/尾,只有内侧一个邻块
        assert!(dimmed(Some(0), 2), "头部活动块,块 2 淡化");
        assert!(!dimmed(Some(0), 1), "头部活动块,块 1 是邻块");
        assert!(dimmed(Some(4), 2), "尾部活动块,块 2 淡化");
        // 无活动块:全部正常
        for index in 0..5 {
            assert!(!dimmed(None, index), "无活动块,块 {index} 不淡化");
        }
    }
}
