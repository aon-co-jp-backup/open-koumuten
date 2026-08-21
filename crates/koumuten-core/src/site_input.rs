//! `open-fudousan`の`GET /v1/properties/:id/export-for-cad`が返す形式
//! (敷地座標・建ぺい率・容積率)を受け取るための入力モデル。
//!
//! `open-fudousan`側の`fudousan-core::export::CadExport`とフィールド
//! 互換になるよう設計した(`open-fudousan`側は今回変更していない)。
//! JSON構造:
//! ```json
//! {
//!   "property_id": "dummy-001",
//!   "site": { "vertices": [[0.0,0.0], [10.0,0.0], [10.0,10.0], [0.0,10.0]] },
//!   "zoning": { "building_coverage_ratio": 0.6, "floor_area_ratio": 2.0 },
//!   "site_area_m2": 100.0,
//!   "is_dummy_data": true,
//!   "road_frontage_m": 4.0
//! }
//! ```
//!
//! **正直な開示・`road_frontage_m`について**: `open-fudousan`の
//! `Property`/`CadExport`には接道義務(建築基準法第43条、敷地が幅員4m
//! 以上の道路に2m以上接すること)を判定するために必要な「道路に接する
//! 辺の長さ」の情報が存在しない(`open-fudousan`側は今回のタスクでは
//! 変更しないという制約のため追加していない)。本クレートでは
//! `road_frontage_m: Option<f64>`という新規フィールドを追加提案し、
//! 未指定(`None`)の場合は接道義務チェックを「判定不能」として明示的に
//! スキップする(黙って適合とみなさない、[`crate::legal_check`]参照)。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CadSiteInput {
    /// ローカル平面座標(メートル単位)の頂点列。`open-fudousan`の
    /// `CadSiteExport::vertices`と同一形式。
    pub vertices: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CadZoningInput {
    pub building_coverage_ratio: f64,
    pub floor_area_ratio: f64,
}

/// `open-koumuten`が受け取る敷地情報。`open-fudousan`の`CadExport`と
/// 互換のフィールドに、本クレート独自の追加提案フィールド
/// (`road_frontage_m`)を加えたもの。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteInput {
    pub property_id: String,
    pub site: CadSiteInput,
    pub zoning: CadZoningInput,
    pub site_area_m2: f64,
    #[serde(default)]
    pub is_dummy_data: bool,
    /// 敷地が道路に接する辺の長さ(メートル)。`open-fudousan`側には
    /// 存在しないため本クレートでの追加提案フィールド。未指定
    /// (`None`)の場合、接道義務チェックは「判定不能」として扱う。
    #[serde(default)]
    pub road_frontage_m: Option<f64>,
}

impl SiteInput {
    /// `cg-cad-core::site::Site`へ変換する。
    pub fn to_cg_cad_site(&self) -> Result<cg_cad_core::site::Site, cg_cad_core::site::SiteError> {
        cg_cad_core::site::Site::polygon(self.site.vertices.clone())
    }

    /// `cg-cad-core::zoning::ZoningRules`へ変換する。
    pub fn to_cg_cad_zoning(&self) -> Result<cg_cad_core::zoning::ZoningRules, cg_cad_core::zoning::ZoningError> {
        cg_cad_core::zoning::ZoningRules::new(self.zoning.building_coverage_ratio, self.zoning.floor_area_ratio)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SiteInput {
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
    fn deserializes_fudousan_export_for_cad_shape() {
        let json = r#"{
            "property_id": "dummy-001",
            "site": { "vertices": [[0.0,0.0], [10.0,0.0], [10.0,10.0], [0.0,10.0]] },
            "zoning": { "building_coverage_ratio": 0.6, "floor_area_ratio": 2.0 },
            "site_area_m2": 100.0,
            "is_dummy_data": true
        }"#;
        let input: SiteInput = serde_json::from_str(json).expect("parses without road_frontage_m");
        assert_eq!(input.property_id, "dummy-001");
        assert_eq!(input.road_frontage_m, None);
    }

    #[test]
    fn converts_to_cg_cad_site_and_zoning() {
        let input = sample();
        let site = input.to_cg_cad_site().unwrap();
        assert!((site.area_m2() - 100.0).abs() < 1e-6);
        let zoning = input.to_cg_cad_zoning().unwrap();
        assert_eq!(zoning.building_coverage_ratio, 0.6);
    }
}
