//! open-koumuten API サーバー。
//!
//! tokio/hyperを直接用いた自前実装(重量級フレームワーク・ORM不使用の
//! エコシステム方針に準拠、`open-raid-z/CLAUDE.md`参照)。
//!
//! # エンドポイント
//! - `POST /v1/proposals` — `open-fudousan`の`export-for-cad`と互換の
//!   敷地情報(JSONボディ)を受け取り、間取り案の生成→違法性チェック→
//!   (適合案のみ)構造計算→LLMコメント生成、まで一括で行う。
//! - `GET /healthz` — ヘルスチェック。
//!
//! **免責事項**: 全レスポンスに`disclaimer`フィールド
//! (`koumuten_core::DISCLAIMER_JA`)を必ず含める——本サービスの提案は
//! 一級建築士等の有資格者による最終確認前のAIドラフトである。
//!
//! **正直な開示・既知の制約**: `POST /v1/proposals`のハンドラ内で
//! `aruaru-llm`へのHTTPリクエストを同期(blocking)の`reqwest`クライアント
//! で行っている。`open-cg-cad::llm_explain`と同じ設計だが、tokioの
//! 非同期ランタイム上でこれを直接呼ぶと、そのリクエストの間はワーカー
//! スレッドがブロックされる(`tokio::task::spawn_blocking`は使っていない)。
//! MVPとして許容したが、高負荷環境では改善の余地がある。

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioExecutor;
use hyper_util::rt::TokioIo;
use hyper_util::server::conn::auto::Builder as ConnBuilder;
use koumuten_core::floor_plan::{generate_proposals, FloorPlanProposal, GenerationOptions};
use koumuten_core::legal_check::{check_proposal as check_legal, LegalCheckResult};
use koumuten_core::llm_comment::generate_comment;
use koumuten_core::site_input::SiteInput;
use koumuten_core::structural_check::check_proposal as check_structural;
use koumuten_core::DISCLAIMER_JA;
use serde::Serialize;
use tokio::net::TcpListener;

type BoxBody = Full<Bytes>;

struct AppState {
    aruaru_llm_base_url: String,
}

fn json_response(status: StatusCode, value: &impl Serialize) -> Response<BoxBody> {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
    Response::builder()
        .status(status)
        .header("content-type", "application/json; charset=utf-8")
        .body(Full::new(Bytes::from(body)))
        .unwrap()
}

fn bad_request(field: &str, message: &str) -> Response<BoxBody> {
    json_response(
        StatusCode::BAD_REQUEST,
        &serde_json::json!({ "error": "bad_request", "field": field, "message": message, "disclaimer": DISCLAIMER_JA }),
    )
}

#[derive(Debug, Serialize)]
struct ProposalEntry {
    proposal: FloorPlanProposal,
    legal_check: LegalCheckResult,
    structural_check: Option<serde_json::Value>,
    llm_comment: Option<String>,
}

#[derive(Debug, Serialize)]
struct ProposalsResponse {
    property_id: String,
    disclaimer: String,
    total_generated: usize,
    compliant_count: usize,
    entries: Vec<ProposalEntry>,
}

/// メソッド・パス・ボディからルーティングを行う同期の純粋関数
/// (fudousan-apiと同じ設計パターン、単体テストで実サーバー起動なしに
/// 検証可能)。
fn route(method: &Method, path: &str, body: &[u8], state: &AppState) -> Response<BoxBody> {
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();

    match (method, segments.as_slice()) {
        (&Method::POST, ["v1", "proposals"]) => handle_proposals(body, state),
        (&Method::GET, ["healthz"]) => json_response(StatusCode::OK, &serde_json::json!({ "status": "ok" })),
        (m, _) if m != Method::POST && m != Method::GET => json_response(
            StatusCode::METHOD_NOT_ALLOWED,
            &serde_json::json!({ "error": "method_not_allowed", "message": format!("unsupported method {m}") }),
        ),
        _ => json_response(
            StatusCode::NOT_FOUND,
            &serde_json::json!({
                "error": "not_found",
                "message": "unknown route; see POST /v1/proposals, GET /healthz",
            }),
        ),
    }
}

fn handle_proposals(body: &[u8], state: &AppState) -> Response<BoxBody> {
    let input: SiteInput = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(err) => return bad_request("body", &format!("invalid JSON body (expected open-fudousan export-for-cad shape + optional road_frontage_m): {err}")),
    };

    if input.site.vertices.len() < 3 {
        return bad_request("site.vertices", "site polygon must have at least 3 vertices");
    }

    let proposals = generate_proposals(&input, &GenerationOptions::default());
    let total_generated = proposals.len();

    let mut entries = Vec::with_capacity(proposals.len());
    let mut compliant_count = 0usize;

    for proposal in proposals {
        let legal = check_legal(&input, &proposal);
        if !legal.passes_checked_items {
            entries.push(ProposalEntry { proposal, legal_check: legal, structural_check: None, llm_comment: None });
            continue;
        }
        compliant_count += 1;

        let structural = match check_structural(&input, &proposal) {
            Ok(outcome) => Some(serde_json::to_value(outcome).unwrap_or(serde_json::Value::Null)),
            Err(err) => Some(serde_json::json!({ "error": err.to_string() })),
        };

        let comment = generate_comment(&state.aruaru_llm_base_url, &proposal);

        entries.push(ProposalEntry { proposal, legal_check: legal, structural_check: structural, llm_comment: Some(comment) });
    }

    json_response(
        StatusCode::OK,
        &ProposalsResponse {
            property_id: input.property_id.clone(),
            disclaimer: DISCLAIMER_JA.to_string(),
            total_generated,
            compliant_count,
            entries,
        },
    )
}

async fn handle(state: Arc<AppState>, req: Request<Incoming>) -> Result<Response<BoxBody>, Infallible> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let body_bytes = req.into_body().collect().await.map(|b| b.to_bytes()).unwrap_or_default();

    Ok(route(&method, &path, &body_bytes, &state))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let aruaru_llm_base_url =
        std::env::var("KOUMUTEN_ARUARU_LLM_URL").unwrap_or_else(|_| "http://127.0.0.1:4600".to_string());
    let state = Arc::new(AppState { aruaru_llm_base_url });

    let port: u16 = std::env::var("KOUMUTEN_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(8090);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;
    eprintln!("open-koumuten API listening on http://{addr} (aruaru-llm base url: {})", state.aruaru_llm_base_url);
    eprintln!("DISCLAIMER: {DISCLAIMER_JA}");

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let state = state.clone();

        tokio::spawn(async move {
            let service = service_fn(move |req| handle(state.clone(), req));
            if let Err(err) = ConnBuilder::new(TokioExecutor::new()).serve_connection(io, service).await {
                eprintln!("connection error: {err}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> AppState {
        // ポート1(通常誰も待受していない)を指すことで、LLM接続が常に
        // 失敗しフォールバック定型文になることを保証し、テストを
        // 決定的にする(aruaru-llmが起動しているかどうかに依存しない)。
        AppState { aruaru_llm_base_url: "http://127.0.0.1:1".to_string() }
    }

    fn sample_body() -> Vec<u8> {
        serde_json::json!({
            "property_id": "dummy-001",
            "site": { "vertices": [[0.0,0.0], [10.0,0.0], [10.0,10.0], [0.0,10.0]] },
            "zoning": { "building_coverage_ratio": 0.6, "floor_area_ratio": 2.0 },
            "site_area_m2": 100.0,
            "is_dummy_data": true,
            "road_frontage_m": 4.0
        })
        .to_string()
        .into_bytes()
    }

    async fn body_json(resp: Response<BoxBody>) -> serde_json::Value {
        let bytes = resp.into_body().collect().await.expect("infallible").to_bytes();
        serde_json::from_slice(&bytes).expect("response body must be JSON")
    }

    #[test]
    fn healthz_returns_ok() {
        let state = test_state();
        let resp = route(&Method::GET, "/healthz", b"", &state);
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn proposals_endpoint_returns_disclaimer_and_entries() {
        let state = test_state();
        let body = sample_body();
        let resp = route(&Method::POST, "/v1/proposals", &body, &state);
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert!(json["disclaimer"].as_str().unwrap().contains("有資格者"));
        assert!(json["total_generated"].as_u64().unwrap() > 0);
        assert!(json["entries"].as_array().unwrap().len() > 0);
    }

    #[tokio::test]
    async fn proposals_endpoint_marks_overshoot_variant_as_non_compliant() {
        let state = test_state();
        let body = sample_body();
        let resp = route(&Method::POST, "/v1/proposals", &body, &state);
        let json = body_json(resp).await;
        let entries = json["entries"].as_array().unwrap();
        let has_violation = entries.iter().any(|e| !e["legal_check"]["violations"].as_array().unwrap().is_empty());
        assert!(has_violation, "expected at least one overshoot variant with violations");
        let rejected_has_no_structural = entries
            .iter()
            .find(|e| !e["legal_check"]["violations"].as_array().unwrap().is_empty())
            .map(|e| e["structural_check"].is_null())
            .unwrap_or(false);
        assert!(rejected_has_no_structural, "non-compliant proposals should skip structural check");
    }

    #[tokio::test]
    async fn proposals_endpoint_compliant_entries_include_fallback_llm_comment() {
        let state = test_state();
        let body = sample_body();
        let resp = route(&Method::POST, "/v1/proposals", &body, &state);
        let json = body_json(resp).await;
        let entries = json["entries"].as_array().unwrap();
        let compliant = entries.iter().find(|e| e["legal_check"]["violations"].as_array().unwrap().is_empty());
        let compliant = compliant.expect("expected at least one compliant proposal");
        assert!(compliant["llm_comment"].as_str().unwrap().contains("定型文"));
    }

    #[test]
    fn invalid_json_body_returns_400() {
        let state = test_state();
        let resp = route(&Method::POST, "/v1/proposals", b"not json", &state);
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn unknown_route_returns_404() {
        let state = test_state();
        let resp = route(&Method::GET, "/nonexistent", b"", &state);
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn wrong_method_returns_405() {
        let state = test_state();
        let resp = route(&Method::DELETE, "/v1/proposals", b"", &state);
        assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
    }
}
