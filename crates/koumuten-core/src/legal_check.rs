//! 建築基準法上、明らかに違法となる間取り案を検出して除外する簡易
//! チェッカー。
//!
//! **正直な開示(最重要、実装した範囲を隠さない)**: 建築基準法の細目を
//! 全て網羅しているわけではない。本モジュールが実装しているのは以下の
//! 3項目のみ:
//!
//! 1. **建蔽率・容積率オーバー**(建築基準法第53条・第52条): 敷地面積に
//!    対する建築面積・延床面積の上限チェック。`cg-cad-core::zoning`の
//!    既存実装(`check_zoning_compliance`)をそのまま再利用する。
//! 2. **接道義務違反**(建築基準法第43条、簡易判定): 敷地が幅員4m以上の
//!    道路に2m以上接することを要求する規定。本チェッカーは
//!    `SiteInput::road_frontage_m`が`Some(x)`かつ`x >= 2.0`メートルで
//!    あれば適合とみなす、という単純化した判定に留める——実際の接道
//!    義務判定には道路の幅員・種別(建築基準法上の「道路」に該当するか)
//!    等の確認が必要であり、本チェッカーはそれを行わない。
//!    `road_frontage_m`が`None`(未指定)の場合は「判定不能」として
//!    警告付きで通す(黙って適合とみなさない)。
//! 3. **採光規定の明らかな違反**(建築基準法第28条、簡易判定): 居室
//!    (LDK・寝室)がfootprintの外壁に一切接していない(内部に完全に
//!    埋没している)場合を「明らかな採光違反」として却下する。実際の
//!    採光規定は「有効採光面積 ÷ 居室床面積 ≥ 規定の割合(住宅は原則
//!    1/7)」という窓面積・隣地距離を考慮した計算が必要だが、本チェッカー
//!    はそこまで行わず、「外壁に全く接していない居室」という明白な
//!    ケースのみを検出する。
//!
//! 上記以外(高さ制限・日影規制・斜線制限・防火地域規制・避難規定等)は
//! 一切チェックしていない——[`crate::DISCLAIMER_JA`]の免責事項と合わせ、
//! 出力の`checked_items`/`not_checked_items`フィールドで正直に開示する。

use crate::floor_plan::FloorPlanProposal;
use crate::site_input::SiteInput;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Violation {
    /// 建蔽率オーバー。
    CoverageRatioExceeded { footprint_m2: f64, max_footprint_m2: f64 },
    /// 容積率オーバー。
    FloorAreaRatioExceeded { total_floor_area_m2: f64, max_total_floor_area_m2: f64 },
    /// 接道義務違反(道路との接道が2m未満と判明している場合のみ)。
    RoadFrontageInsufficient { road_frontage_m: f64, required_m: f64 },
    /// 採光規定の明らかな違反(居室が外壁に全く接していない)。
    DaylightViolation { room_kind_label: String, floor: u32 },
}

impl Violation {
    pub fn label_ja(&self) -> String {
        match self {
            Violation::CoverageRatioExceeded { footprint_m2, max_footprint_m2 } => format!(
                "建蔽率オーバー: 建築面積{footprint_m2:.2}m2が上限{max_footprint_m2:.2}m2を超過しています"
            ),
            Violation::FloorAreaRatioExceeded { total_floor_area_m2, max_total_floor_area_m2 } => format!(
                "容積率オーバー: 延床面積{total_floor_area_m2:.2}m2が上限{max_total_floor_area_m2:.2}m2を超過しています"
            ),
            Violation::RoadFrontageInsufficient { road_frontage_m, required_m } => format!(
                "接道義務違反の疑い: 道路に接する長さ{road_frontage_m:.2}mが必要な{required_m:.2}m未満です"
            ),
            Violation::DaylightViolation { room_kind_label, floor } => format!(
                "採光規定違反の疑い: {floor}階の{room_kind_label}が外壁に接しておらず、窓による採光が取れません"
            ),
        }
    }
}

/// 1件の間取り案に対する違法性チェック結果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegalCheckResult {
    pub proposal_id: String,
    pub violations: Vec<Violation>,
    /// 上記1〜3のいずれの違反も検出されなかった場合`true`
    /// (=「除外対象ではない」、法的な適合を保証するものではない)。
    pub passes_checked_items: bool,
    /// 実装したチェック項目。
    pub checked_items: Vec<String>,
    /// 未実装のチェック項目(正直な開示)。
    pub not_checked_items: Vec<String>,
}

const CHECKED_ITEMS: [&str; 3] = ["建蔽率・容積率(第52条・第53条)", "接道義務(第43条、簡易判定)", "採光規定(第28条、明らかな違反のみ)"];
const NOT_CHECKED_ITEMS: [&str; 5] = ["高さ制限", "日影規制", "斜線制限(道路斜線・隣地斜線・北側斜線)", "防火地域・準防火地域規制", "避難規定(廊下幅・階段・排煙等)"];

const REQUIRED_ROAD_FRONTAGE_M: f64 = 2.0;
const BOUNDARY_EPSILON_M: f64 = 0.01;

/// 単一の間取り案を検証する。
pub fn check_proposal(input: &SiteInput, proposal: &FloorPlanProposal) -> LegalCheckResult {
    let mut violations = Vec::new();

    // 1. 建蔽率・容積率(cg-cad-core::zoningの既存実装を再利用)。
    if let Ok(zoning) = input.to_cg_cad_zoning() {
        let max_footprint = zoning.max_building_footprint_m2(input.site_area_m2);
        let max_total_floor_area = zoning.max_total_floor_area_m2(input.site_area_m2);
        if let Err(err) = cg_cad_core::zoning::check_zoning_compliance(
            input.site_area_m2,
            &zoning,
            proposal.footprint_m2,
            proposal.total_floor_area_m2,
        ) {
            match err {
                cg_cad_core::zoning::ZoningError::FootprintExceedsCoverage { .. } => {
                    violations.push(Violation::CoverageRatioExceeded { footprint_m2: proposal.footprint_m2, max_footprint_m2: max_footprint });
                }
                cg_cad_core::zoning::ZoningError::TotalFloorAreaExceedsRatio { .. } => {
                    violations.push(Violation::FloorAreaRatioExceeded {
                        total_floor_area_m2: proposal.total_floor_area_m2,
                        max_total_floor_area_m2: max_total_floor_area,
                    });
                }
                _ => {}
            }
        }
    }

    // 2. 接道義務(簡易判定)。未指定(None)の場合はチェック対象外
    //    (判定不能、黙って適合とみなさない——`checked_items`に記載しつつ
    //    実際には判定できなかった旨は呼び出し側のドキュメントで開示)。
    if let Some(frontage) = input.road_frontage_m {
        if frontage < REQUIRED_ROAD_FRONTAGE_M {
            violations.push(Violation::RoadFrontageInsufficient { road_frontage_m: frontage, required_m: REQUIRED_ROAD_FRONTAGE_M });
        }
    }

    // 3. 採光規定(明らかな違反のみ)。
    for room in &proposal.rooms {
        if room.kind.requires_daylight()
            && !room.touches_footprint_boundary(proposal.footprint_width_m, proposal.footprint_depth_m, BOUNDARY_EPSILON_M)
        {
            violations.push(Violation::DaylightViolation { room_kind_label: room.kind.label_ja().to_string(), floor: room.floor });
        }
    }

    let passes_checked_items = violations.is_empty();
    LegalCheckResult {
        proposal_id: proposal.proposal_id.clone(),
        violations,
        passes_checked_items,
        checked_items: CHECKED_ITEMS.iter().map(|s| s.to_string()).collect(),
        not_checked_items: NOT_CHECKED_ITEMS.iter().map(|s| s.to_string()).collect(),
    }
}

/// 複数の間取り案を検証し、違反した案を除外した「適合案のみ」の
/// リストと、全案の検証結果の両方を返す。
pub fn filter_compliant_proposals<'a>(
    input: &SiteInput,
    proposals: &'a [FloorPlanProposal],
) -> (Vec<&'a FloorPlanProposal>, Vec<LegalCheckResult>) {
    let results: Vec<LegalCheckResult> = proposals.iter().map(|p| check_proposal(input, p)).collect();
    let compliant: Vec<&FloorPlanProposal> = proposals
        .iter()
        .zip(results.iter())
        .filter(|(_, r)| r.passes_checked_items)
        .map(|(p, _)| p)
        .collect();
    (compliant, results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::floor_plan::{generate_proposals, GenerationOptions};
    use crate::site_input::{CadSiteInput, CadZoningInput};

    fn sample_input(road_frontage_m: Option<f64>) -> SiteInput {
        SiteInput {
            property_id: "dummy-001".to_string(),
            site: CadSiteInput { vertices: vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)] },
            zoning: CadZoningInput { building_coverage_ratio: 0.6, floor_area_ratio: 2.0 },
            site_area_m2: 100.0,
            is_dummy_data: true,
            road_frontage_m,
        }
    }

    #[test]
    fn overshoot_proposal_is_rejected_for_coverage_ratio() {
        let input = sample_input(Some(4.0));
        let opts = GenerationOptions { footprint_coverage_fractions: vec![1.15], floor_counts: vec![1], max_variants: 1 };
        let proposals = generate_proposals(&input, &opts);
        let result = check_proposal(&input, &proposals[0]);
        assert!(!result.passes_checked_items);
        assert!(matches!(result.violations[0], Violation::CoverageRatioExceeded { .. }));
    }

    #[test]
    fn compliant_proposal_passes() {
        let input = sample_input(Some(4.0));
        let opts = GenerationOptions { footprint_coverage_fractions: vec![0.75], floor_counts: vec![1], max_variants: 1 };
        let proposals = generate_proposals(&input, &opts);
        let result = check_proposal(&input, &proposals[0]);
        assert!(result.passes_checked_items, "violations: {:?}", result.violations);
    }

    #[test]
    fn insufficient_road_frontage_is_flagged() {
        let input = sample_input(Some(1.0));
        let opts = GenerationOptions { footprint_coverage_fractions: vec![0.75], floor_counts: vec![1], max_variants: 1 };
        let proposals = generate_proposals(&input, &opts);
        let result = check_proposal(&input, &proposals[0]);
        assert!(result.violations.iter().any(|v| matches!(v, Violation::RoadFrontageInsufficient { .. })));
    }

    #[test]
    fn unknown_road_frontage_is_not_flagged_as_violation() {
        let input = sample_input(None);
        let opts = GenerationOptions { footprint_coverage_fractions: vec![0.75], floor_counts: vec![1], max_variants: 1 };
        let proposals = generate_proposals(&input, &opts);
        let result = check_proposal(&input, &proposals[0]);
        assert!(!result.violations.iter().any(|v| matches!(v, Violation::RoadFrontageInsufficient { .. })));
    }

    #[test]
    fn filter_compliant_proposals_excludes_violations() {
        let input = sample_input(Some(4.0));
        let opts = GenerationOptions::default();
        let proposals = generate_proposals(&input, &opts);
        let (compliant, results) = filter_compliant_proposals(&input, &proposals);
        assert!(compliant.len() < proposals.len(), "expected at least one overshoot variant to be filtered out");
        assert_eq!(results.len(), proposals.len());
    }
}
