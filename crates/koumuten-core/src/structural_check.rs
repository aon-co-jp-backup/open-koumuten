//! `open-cg-cad`(`cg-cad-core`)の構造計算・耐震シミュレーションAPIを
//! 呼び出し、間取り案が構造的に成立するかの簡易検証を行う。
//!
//! **連携方式(正直な開示)**: `open-cg-cad`は現時点でHTTP APIを公開して
//! おらず(`cg-cad-core`クレートのみ)、`open-fudousan`側のHANDOFFに
//! 記録されている通り実プロセス間連携用のエンドポイントが存在しない。
//! そのため本モジュールは、タスク指示にある2つの選択肢
//! (「実プロセス間HTTP」または「直接crate依存」)のうち**crate依存**を
//! 採用し、`cg-cad-core`をpath依存として直接呼び出す(`Cargo.toml`参照)。
//!
//! **簡略化している点**: `cg-cad-core::building::generate_building`は
//! 敷地の外接矩形を建蔽率上限に収まるようスケールして建物footprintを
//! 独自に算出する設計になっており、[`crate::floor_plan`]が生成する
//! 間取り案の`footprint_m2`と厳密には一致しない(同じ敷地・建蔽率・
//! 容積率・階数から`cg-cad-core`自身が導出したfootprintで再生成する形)。
//! そのため本チェックは「この間取り案とピクセル単位で同一の建物」の
//! 構造計算ではなく、「同じ敷地条件・階数から`cg-cad-core`が生成する
//! 代表的な建物ボリューム」に対する構造計算である、という近似である
//! ことを正直に開示する。

use crate::floor_plan::FloorPlanProposal;
use crate::site_input::SiteInput;
use cg_cad_core::building::{generate_building, BuildingType, CadError};
use cg_cad_core::structural::{analyze_structure, StructuralAnalysisResult, StructuralError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StructuralCheckError {
    #[error("site polygon error: {0}")]
    Site(#[from] cg_cad_core::site::SiteError),
    #[error("zoning input error: {0}")]
    Zoning(#[from] cg_cad_core::zoning::ZoningError),
    #[error("building generation error: {0}")]
    Building(#[from] CadError),
    #[error("structural analysis error: {0}")]
    Structural(#[from] StructuralError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuralCheckOutcome {
    pub proposal_id: String,
    /// 震度係数(既定0.3、震度6強相当の簡易近似——`cg-cad-core`側の
    /// ドキュメントに準じる目安)。
    pub seismic_coefficient: f64,
    pub all_within_limit: bool,
    pub drift_limit: f64,
    pub story_count: usize,
    pub disclaimer: String,
}

/// 既定の震度係数(震度6強相当の簡易近似、`cg-cad-core::structural`の
/// ドキュメントコメントが例示する0.3〜0.4の下限値を採用)。
pub const DEFAULT_SEISMIC_COEFFICIENT: f64 = 0.3;

/// 間取り案に対して構造計算・耐震シミュレーションを実行する。
pub fn check_proposal(input: &SiteInput, proposal: &FloorPlanProposal) -> Result<StructuralCheckOutcome, StructuralCheckError> {
    let site = input.to_cg_cad_site()?;
    let zoning = input.to_cg_cad_zoning()?;
    let building = generate_building(&site, &zoning, BuildingType::House, proposal.floors)?;
    let result: StructuralAnalysisResult = analyze_structure(&building, DEFAULT_SEISMIC_COEFFICIENT, None)?;

    Ok(StructuralCheckOutcome {
        proposal_id: proposal.proposal_id.clone(),
        seismic_coefficient: DEFAULT_SEISMIC_COEFFICIENT,
        all_within_limit: result.all_within_limit,
        drift_limit: result.drift_limit,
        story_count: result.story_drifts.len(),
        disclaimer: result.disclaimer.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::floor_plan::{generate_proposals, GenerationOptions};
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
    fn structural_check_succeeds_for_compliant_proposal() {
        let input = sample_input();
        let opts = GenerationOptions { footprint_coverage_fractions: vec![0.75], floor_counts: vec![2], max_variants: 1 };
        let proposals = generate_proposals(&input, &opts);
        let outcome = check_proposal(&input, &proposals[0]).expect("structural check should succeed");
        assert_eq!(outcome.story_count, 2);
        assert!(outcome.drift_limit > 0.0);
    }

    #[test]
    fn structural_check_fails_cleanly_for_zero_floor_input() {
        let input = sample_input();
        let bogus = FloorPlanProposal {
            proposal_id: "bogus".to_string(),
            floors: 0,
            footprint_m2: 60.0,
            footprint_width_m: 7.7,
            footprint_depth_m: 7.7,
            total_floor_area_m2: 0.0,
            rooms: vec![],
        };
        let result = check_proposal(&input, &bogus);
        assert!(result.is_err());
    }
}
