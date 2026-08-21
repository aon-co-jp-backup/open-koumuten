//! `aruaru-llm`の`POST /v1/generate`を使い、間取り提案のユーザー向け
//! 自然文コメントを生成する。接続失敗時は定型文へフォールバックする
//! (`aruaru-llm`が起動していなくても呼び出し側の動作確認ができるように
//! するため)。
//!
//! **正直な開示**: `open-cg-cad::llm_explain`と同じ設計パターン
//! (`reqwest`のblockingクライアントで`POST /v1/generate`を呼ぶ)を踏襲
//! している。GPT-2 124M(対話ファインチューニング無し)を使うため、
//! 生成される文言の品質・一貫性は保証されない——`aruaru-llm`自身が
//! 明記している限界と同じ。

use crate::floor_plan::FloorPlanProposal;
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmCommentError {
    #[error("aruaru-llm request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("aruaru-llm returned an error response: {0}")]
    ErrorResponse(String),
}

#[derive(Debug, Deserialize)]
struct GenerateResponse {
    completion: String,
}

#[derive(Debug, Deserialize)]
struct GenerateErrorResponse {
    error: String,
}

/// 間取り案の説明文を`aruaru-llm`向けの英語プロンプトへ整形する
/// (GPT-2はBPE語彙が英語中心のため英語プロンプトを使う、
/// `open-cg-cad::llm_explain`と同じ判断)。
pub fn build_prompt(proposal: &FloorPlanProposal) -> String {
    format!(
        "Describe this house floor plan proposal to a customer in one friendly sentence. \
Do not invent facts not given here. Facts: {} floor(s), {:.1} square meters footprint, \
{:.1} square meters total floor area, {} rooms planned. Response:",
        proposal.floors,
        proposal.footprint_m2,
        proposal.total_floor_area_m2,
        proposal.rooms.len()
    )
}

/// 接続失敗・応答異常時に使う定型文(日本語)。
pub fn fallback_comment_ja(proposal: &FloorPlanProposal) -> String {
    format!(
        "{}階建て、延床面積約{:.1}平米、部屋数{}室のプランをご提案しました。\
(aruaru-llmサーバーに接続できなかったため、この文章は定型文です。)",
        proposal.floors,
        proposal.total_floor_area_m2,
        proposal.rooms.len()
    )
}

/// `aruaru-llm`の`POST /v1/generate`を呼び出し、提案コメントを取得する。
/// 接続失敗・エラー応答の場合は`Err`ではなく`fallback_comment_ja`を返す
/// (呼び出し側がaruaru-llm未起動でも動作確認できるようにするため)。
pub fn generate_comment(base_url: &str, proposal: &FloorPlanProposal) -> String {
    match try_generate_comment(base_url, proposal) {
        Ok(text) => text,
        Err(_) => fallback_comment_ja(proposal),
    }
}

fn try_generate_comment(base_url: &str, proposal: &FloorPlanProposal) -> Result<String, LlmCommentError> {
    let prompt = build_prompt(proposal);
    let client = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(30)).build()?;
    let response = client
        .post(format!("{base_url}/v1/generate"))
        .json(&serde_json::json!({ "prompt": prompt, "max_new_tokens": 32 }))
        .send()?;

    if !response.status().is_success() {
        let err: Result<GenerateErrorResponse, _> = response.json();
        return Err(LlmCommentError::ErrorResponse(
            err.map(|e| e.error).unwrap_or_else(|_| "unknown error".to_string()),
        ));
    }

    let body: GenerateResponse = response.json()?;
    Ok(format!(
        "この様なデザインで素敵なお家を立ててみました。{}",
        body.completion.trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::floor_plan::Room;
    use crate::floor_plan::RoomKind;

    fn sample_proposal() -> FloorPlanProposal {
        FloorPlanProposal {
            proposal_id: "plan-1f-100pct".to_string(),
            floors: 1,
            footprint_m2: 60.0,
            footprint_width_m: 7.75,
            footprint_depth_m: 7.75,
            total_floor_area_m2: 60.0,
            rooms: vec![Room { kind: RoomKind::LivingDiningKitchen, floor: 1, origin: (0.0, 0.0), width_m: 7.75, depth_m: 3.87 }],
        }
    }

    #[test]
    fn build_prompt_includes_facts_only() {
        let p = sample_proposal();
        let prompt = build_prompt(&p);
        assert!(prompt.contains("1 floor"));
        assert!(prompt.contains("60.0 square meters"));
    }

    #[test]
    fn fallback_comment_mentions_disclosure_of_being_a_fallback() {
        let p = sample_proposal();
        let comment = fallback_comment_ja(&p);
        assert!(comment.contains("定型文"));
    }

    #[test]
    fn generate_comment_falls_back_when_server_is_unreachable() {
        let p = sample_proposal();
        // ポート1番(通常誰も待受していない予約済みポート)を使い、
        // 接続失敗を確実に再現する。
        let comment = generate_comment("http://127.0.0.1:1", &p);
        assert!(comment.contains("定型文"));
    }
}
