//! 敷地情報から複数の間取り案(部屋数・配置・延床面積)を生成する。
//!
//! **正直な開示(簡略化している点)**: 実際の間取り設計(動線計画・
//! 採光方向・構造壁の配置等)は行わない——各室の「配置」は敷地の外接
//! 矩形を格子状に単純分割した矩形の羅列(グリッドレイアウト)に留まる、
//! 意匠検討の叩き台レベルのドラフト。部屋の形状は全て矩形。

use crate::site_input::SiteInput;
use serde::{Deserialize, Serialize};

/// 生成する部屋の用途(簡易分類、実際の建築計画の全カテゴリを網羅しない)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoomKind {
    LivingDiningKitchen,
    Bedroom,
    Bathroom,
    Toilet,
    Entrance,
    Storage,
}

impl RoomKind {
    pub fn label_ja(&self) -> &'static str {
        match self {
            RoomKind::LivingDiningKitchen => "LDK(リビング・ダイニング・キッチン)",
            RoomKind::Bedroom => "寝室",
            RoomKind::Bathroom => "浴室",
            RoomKind::Toilet => "トイレ",
            RoomKind::Entrance => "玄関",
            RoomKind::Storage => "収納",
        }
    }

    /// この部屋が採光(建築基準法上の居室)を必要とするかどうかの簡易分類。
    /// **正直な開示**: 実際の採光規定(法28条)は居室全般に適用され、
    /// 用途・面積により有効採光面積の計算が必要だが、本クレートでは
    /// 「居室(LDK・寝室)は外壁に面する必要がある」という簡易ルールに
    /// 留める([`crate::legal_check`]参照)。
    pub fn requires_daylight(&self) -> bool {
        matches!(self, RoomKind::LivingDiningKitchen | RoomKind::Bedroom)
    }
}

/// 1部屋分の矩形配置。座標はローカル平面座標(メートル)、建物footprint内
/// のグリッド分割による単純な配置。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Room {
    pub kind: RoomKind,
    pub floor: u32,
    /// 部屋矩形の左下角(x, y)、footprintローカル座標系(メートル)。
    pub origin: (f64, f64),
    pub width_m: f64,
    pub depth_m: f64,
}

impl Room {
    pub fn area_m2(&self) -> f64 {
        self.width_m * self.depth_m
    }

    /// この部屋が建物外壁(footprintの境界)に接しているか(採光チェック用の
    /// 簡易判定に使う、[`crate::legal_check`]参照)。
    pub fn touches_footprint_boundary(&self, footprint_width_m: f64, footprint_depth_m: f64, epsilon: f64) -> bool {
        let (x, y) = self.origin;
        x <= epsilon
            || y <= epsilon
            || (x + self.width_m) >= footprint_width_m - epsilon
            || (y + self.depth_m) >= footprint_depth_m - epsilon
    }
}

/// 1つの間取り案。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FloorPlanProposal {
    pub proposal_id: String,
    pub floors: u32,
    /// 建築面積(1階の設置面積、平方メートル)。
    pub footprint_m2: f64,
    pub footprint_width_m: f64,
    pub footprint_depth_m: f64,
    /// 延床面積(全階合計、平方メートル)。
    pub total_floor_area_m2: f64,
    pub rooms: Vec<Room>,
}

/// 間取り案生成の設定。
#[derive(Debug, Clone, PartialEq)]
pub struct GenerationOptions {
    /// 生成する案の数(実際には`floors`のバリエーション×
    /// `footprint_coverage_fractions`の組み合わせ数まで)。
    pub max_variants: usize,
    /// 建蔽率上限に対する建築面積の割合の候補(例: [1.0, 0.8, 1.15]は
    /// 「上限ぴったり」「余裕を持たせた案」「意図的に上限超過させた案」の
    /// 3種——最後の意図的超過案は[`crate::legal_check`]が正しく除外
    /// できることを実証するために生成する)。
    pub footprint_coverage_fractions: Vec<f64>,
    /// 検討する階数の候補。
    pub floor_counts: Vec<u32>,
}

impl Default for GenerationOptions {
    fn default() -> Self {
        GenerationOptions {
            max_variants: 6,
            footprint_coverage_fractions: vec![1.0, 0.75, 1.15],
            floor_counts: vec![1, 2],
        }
    }
}

/// 敷地情報から複数の間取り案を生成する。
///
/// 手順(簡易):
/// 1. 敷地の外接矩形を求め、`footprint_coverage_fractions`各値に対して
///    建蔽率上限(またはその一部/超過分)に収まる(または意図的に超過する)
///    footprintを算出する。
/// 2. `floor_counts`各値に対して延床面積を算出する。
/// 3. footprintを部屋数に応じてグリッド分割し、部屋を配置する
///    (LDK1室・寝室(延床面積に応じて1〜3室)・浴室・トイレ・玄関・収納)。
pub fn generate_proposals(input: &SiteInput, options: &GenerationOptions) -> Vec<FloorPlanProposal> {
    let site = match input.to_cg_cad_site() {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let ((min_x, min_y), (max_x, max_y)) = site.bounding_box();
    let bbox_width = (max_x - min_x).max(0.001);
    let bbox_depth = (max_y - min_y).max(0.001);
    let bbox_area = bbox_width * bbox_depth;

    let max_footprint = input.zoning.building_coverage_ratio * input.site_area_m2;

    let mut proposals = Vec::new();
    let mut count = 0usize;
    for &floors in &options.floor_counts {
        for &fraction in &options.footprint_coverage_fractions {
            if count >= options.max_variants {
                break;
            }
            let target_footprint = (max_footprint * fraction).min(bbox_area);
            if target_footprint <= 0.0 {
                continue;
            }
            let scale = (target_footprint / bbox_area).sqrt();
            let footprint_width_m = bbox_width * scale;
            let footprint_depth_m = bbox_depth * scale;
            let footprint_m2 = footprint_width_m * footprint_depth_m;
            let total_floor_area_m2 = footprint_m2 * floors as f64;

            let rooms = layout_rooms(floors, footprint_width_m, footprint_depth_m);

            let proposal_id = format!("plan-{}f-{:.0}pct", floors, fraction * 100.0);
            proposals.push(FloorPlanProposal {
                proposal_id,
                floors,
                footprint_m2,
                footprint_width_m,
                footprint_depth_m,
                total_floor_area_m2,
                rooms,
            });
            count += 1;
        }
    }
    proposals
}

/// 各階を2x2グリッドへ単純分割し、部屋を割り当てる
/// (実際の動線・採光方向を考慮した精緻な配置ではない、簡易ドラフト)。
fn layout_rooms(floors: u32, footprint_width_m: f64, footprint_depth_m: f64) -> Vec<Room> {
    let mut rooms = Vec::new();
    let half_w = footprint_width_m / 2.0;
    let half_d = footprint_depth_m / 2.0;

    for floor in 1..=floors {
        if floor == 1 {
            // 1階: LDK(半分)+玄関+トイレ+浴室(残り半分を3分割)。
            rooms.push(Room { kind: RoomKind::LivingDiningKitchen, floor, origin: (0.0, 0.0), width_m: footprint_width_m, depth_m: half_d });
            let sub_w = footprint_width_m / 3.0;
            rooms.push(Room { kind: RoomKind::Entrance, floor, origin: (0.0, half_d), width_m: sub_w, depth_m: half_d });
            rooms.push(Room { kind: RoomKind::Toilet, floor, origin: (sub_w, half_d), width_m: sub_w, depth_m: half_d });
            rooms.push(Room { kind: RoomKind::Bathroom, floor, origin: (sub_w * 2.0, half_d), width_m: sub_w, depth_m: half_d });
        } else {
            // 2階以上: 寝室2室+収納。
            rooms.push(Room { kind: RoomKind::Bedroom, floor, origin: (0.0, 0.0), width_m: half_w, depth_m: footprint_depth_m });
            rooms.push(Room { kind: RoomKind::Bedroom, floor, origin: (half_w, 0.0), width_m: half_w, depth_m: half_d });
            rooms.push(Room { kind: RoomKind::Storage, floor, origin: (half_w, half_d), width_m: half_w, depth_m: half_d });
        }
    }
    rooms
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::site_input::{CadSiteInput, CadZoningInput};

    fn sample_input() -> SiteInput {
        SiteInput {
            property_id: "dummy-001".to_string(),
            site: CadSiteInput { vertices: vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)] },
            zoning: CadZoningInput { building_coverage_ratio: 0.6, floor_area_ratio: 2.0 },
            site_area_m2: 100.0,
            is_dummy_data: true,
            road_frontage_m: Some(4.0),
        }
    }

    #[test]
    fn generates_multiple_proposals() {
        let input = sample_input();
        let proposals = generate_proposals(&input, &GenerationOptions::default());
        assert!(!proposals.is_empty());
        assert!(proposals.len() <= GenerationOptions::default().max_variants);
    }

    #[test]
    fn footprint_100_percent_fraction_matches_max_coverage() {
        let input = sample_input();
        let opts = GenerationOptions { footprint_coverage_fractions: vec![1.0], floor_counts: vec![1], max_variants: 1 };
        let proposals = generate_proposals(&input, &opts);
        assert_eq!(proposals.len(), 1);
        let max_footprint = input.zoning.building_coverage_ratio * input.site_area_m2;
        assert!((proposals[0].footprint_m2 - max_footprint).abs() < 1e-6);
    }

    #[test]
    fn overshoot_fraction_produces_footprint_exceeding_coverage_limit() {
        let input = sample_input();
        let opts = GenerationOptions { footprint_coverage_fractions: vec![1.15], floor_counts: vec![1], max_variants: 1 };
        let proposals = generate_proposals(&input, &opts);
        let max_footprint = input.zoning.building_coverage_ratio * input.site_area_m2;
        assert!(proposals[0].footprint_m2 > max_footprint);
    }

    #[test]
    fn two_floor_plan_has_bedrooms_on_second_floor() {
        let input = sample_input();
        let opts = GenerationOptions { footprint_coverage_fractions: vec![1.0], floor_counts: vec![2], max_variants: 1 };
        let proposals = generate_proposals(&input, &opts);
        let second_floor_bedrooms = proposals[0].rooms.iter().filter(|r| r.floor == 2 && r.kind == RoomKind::Bedroom).count();
        assert_eq!(second_floor_bedrooms, 2);
    }
}
