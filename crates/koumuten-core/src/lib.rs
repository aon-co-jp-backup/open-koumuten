//! open-koumuten コアロジック: AI間取り提案・違法性チェックシミュレーション。
//!
//! # ⚠️ 最重要免責事項(必ず全出力に明記すること)
//!
//! **本クレートが生成する間取り提案は、あくまで一級建築士等の有資格者に
//! よる最終確認の一歩手前までを代行するドラフト/サンプル生成である。
//! 「法的に確定した設計」「建築確認申請に使える正式な図面・仕様書」を
//! 生成するものでは断じてなく、必ず有資格者のレビューが必要。**
//! この文言は[`DISCLAIMER_JA`]として全レスポンス(API/CLI)に必ず含める。
//!
//! # このクレートの役割(段階1、スコープ限定)
//!
//! ユーザー指示により、今回のスコープは「AI間取り提案・違法性チェック
//! シミュレーション」に限定する。電子契約・決済・簿記機能は次フェーズ
//! であり、本クレート・本リポジトリには一切実装しない。
//!
//! 1. [`site_input`]: `open-fudousan`の`export-for-cad`エンドポイントが
//!    返す形式(敷地座標・建ぺい率・容積率)を受け取るための入力モデル。
//!    `open-fudousan`側は今回変更していない——本クレート側で受信専用の
//!    型を新規に用意した。
//! 2. [`floor_plan`]: 敷地に対して複数の間取り案(部屋数・配置・延床
//!    面積)を生成する。
//! 3. [`legal_check`]: 建築基準法上、明らかに違法となる間取り案を検出
//!    して除外する簡易チェッカー(建蔽率・容積率オーバー、接道義務違反、
//!    採光規定の明らかな違反)。**全ての建築基準法の細目を網羅している
//!    わけではない**——実装した範囲は[`legal_check`]モジュールの
//!    ドキュメントに正直に明記する。
//! 4. [`structural_check`]: `open-cg-cad`(`cg-cad-core`)の構造計算・
//!    耐震シミュレーションAPIを呼び出し、間取り案が構造的に成立するかの
//!    簡易検証を行う(同一プロセス内のcrate依存、実プロセス間通信では
//!    ない)。
//! 5. [`llm_comment`]: `aruaru-llm`の`POST /v1/generate`を使い、提案文言
//!    (ユーザー向けの自然文コメント)を生成する。接続失敗時は定型文へ
//!    フォールバックする。

pub mod floor_plan;
pub mod legal_check;
pub mod llm_comment;
pub mod site_input;
pub mod structural_check;

/// 全出力(API/CLI)に必ず含める免責文言。
pub const DISCLAIMER_JA: &str = "この提案は一級建築士等の有資格者による最終確認の一歩手前までを\
代行するAIドラフトであり、建築確認申請等に使える法的に確定した設計ではありません。\
必ず有資格者のレビューを受けてください。";

pub const DISCLAIMER_EN: &str = "This proposal is an AI-generated draft that stops short of \
final review by a licensed architect or equivalent professional. It is NOT a legally finalized \
design suitable for a building permit application. A licensed professional's review is required.";
