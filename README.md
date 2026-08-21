# open-koumuten

*English*: [README-English.md](README-English.md)

AI工務店(仮称)。**AI間取り提案・違法性チェックシミュレーション**を行う
Rust製MVP。姉妹リポジトリ[open-fudousan](https://github.com/aon-co-jp/open-fudousan)
(土地・物件検索)から敷地情報を受け取り、[open-cg-cad](https://github.com/aon-co-jp/open-cg-cad)
(構造計算・耐震シミュレーション)・[aruaru-llm](https://github.com/aon-co-jp/aruaru-llm)
(LLM推論サーバー)と連携する。

## ⚠️ 重要な免責事項(必読)

**本システムが生成する間取り提案は、あくまで一級建築士など人間の有資格者
による最終確認の一歩手前までを代行するドラフト/サンプル生成である。
「法的に確定した設計」「建築確認申請に使える正式な図面・仕様書」を生成
するものでは断じてなく、必ず有資格者のレビューが必要。**この免責文言は
`koumuten_core::DISCLAIMER_JA`として全APIレスポンスの`disclaimer`
フィールドに必ず含まれる。

## 今回のスコープ(段階的開発の第1フェーズ)

ユーザー指示により、今回は「AI間取り提案・違法性チェックシミュレーション」
**のみ**に限定する。**電子契約・決済・簿記機能は次フェーズであり、本
リポジトリには一切実装していない**(スコープ外、下記「見送った部分」参照)。

## アーキテクチャ

```
open-koumuten/
├── Cargo.toml (workspace)
├── crates/
│   ├── koumuten-core/   # 間取り生成・違法性チェック・構造連携・LLM連携のコアロジック
│   └── koumuten-api/    # tokio/hyper直接実装のHTTP API(フレームワーク非依存)
```

- **DB非依存・1バイナリ完結・テンプレートエンジン不使用**の方針
  (エコシステム共通、`open-raid-z/CLAUDE.md`参照)。
- `koumuten-core`は`cg-cad-core`(`open-cg-cad`)を**path依存**として
  直接呼び出す(同一プロセス内、実プロセス間HTTP通信ではない——理由は
  下記「open-cg-cadとの連携」参照)。
- `koumuten-core::llm_comment`は`aruaru-llm`の`POST /v1/generate`を
  **実プロセス間HTTP**(`reqwest` blocking)で呼び出す。接続失敗時は
  定型文へ安全にフォールバックする(`aruaru-llm`未起動でも動作確認可能)。

## ビルド・実行

```sh
cargo build --workspace --release
cargo test --workspace --release
./target/release/koumuten-api   # デフォルトで http://127.0.0.1:8090 を待受
                                 # (KOUMUTEN_PORT環境変数で変更可)
```

`aruaru-llm`のURLは既定`http://127.0.0.1:4600`
(`KOUMUTEN_ARUARU_LLM_URL`環境変数で変更可)。`aruaru-llm`が起動していない
場合でも`koumuten-api`は正常に動作し、`llm_comment`フィールドには
接続失敗を明記した定型文が入る。

## API

- `POST /v1/proposals` — 敷地情報(下記「入力形式」参照)を受け取り、
  複数の間取り案を生成し、違法性チェック→(適合案のみ)構造計算→
  LLMコメント生成、まで一括で行う。
- `GET /healthz` — ヘルスチェック。

### 入力形式(`open-fudousan`の`export-for-cad`と互換)

`open-fudousan`の`GET /v1/properties/:id/export-for-cad`が返す
JSON構造とフィールド互換になるよう設計した(`open-fudousan`側は今回
変更していない)。加えて、`open-fudousan`側に存在しない
**`road_frontage_m`(道路に接する辺の長さ、メートル)を本リポジトリ側で
新規に追加提案**した(接道義務チェックに必要、未指定なら判定をスキップ
する設計、詳細は`koumuten-core::site_input`のドキュメント参照)。

```json
{
  "property_id": "dummy-001",
  "site": { "vertices": [[0.0,0.0], [10.0,0.0], [10.0,10.0], [0.0,10.0]] },
  "zoning": { "building_coverage_ratio": 0.6, "floor_area_ratio": 2.0 },
  "site_area_m2": 100.0,
  "is_dummy_data": true,
  "road_frontage_m": 4.0
}
```

### 実HTTP検証例

```sh
curl -X POST http://127.0.0.1:8090/v1/proposals \
  -H "Content-Type: application/json" \
  -d '{"property_id":"dummy-001","site":{"vertices":[[0,0],[10,0],[10,10],[0,10]]},"zoning":{"building_coverage_ratio":0.6,"floor_area_ratio":2.0},"site_area_m2":100.0,"is_dummy_data":true,"road_frontage_m":4.0}'
```

## 実装内容(段階的スコープの範囲を正直に開示)

1. **間取り案生成**(`koumuten-core::floor_plan`): 敷地の外接矩形・
   建蔽率・容積率・階数の組み合わせから複数案を生成する。部屋配置は
   footprintを単純なグリッド分割した矩形の羅列に留まる
   (動線計画・採光方向等を考慮した精緻な設計ではない、意匠検討の
   叩き台レベルのドラフト)。
2. **違法性チェック**(`koumuten-core::legal_check`): 以下3項目のみを
   実装(**建築基準法の全ての細目を網羅していない**、正直な開示):
   - 建蔽率・容積率オーバー(第52条・第53条、`cg-cad-core::zoning`の
     既存実装を再利用)
   - 接道義務違反の簡易判定(第43条、`road_frontage_m >= 2.0m`のみを
     見る単純化、道路幅員・種別の確認は行わない)
   - 採光規定の明らかな違反の簡易判定(第28条、居室がfootprintの外壁に
     一切接していない場合のみを検出、有効採光面積計算等は行わない)
3. **構造計算連携**(`koumuten-core::structural_check`): `cg-cad-core`の
   `generate_building`+`analyze_structure`をpath依存で直接呼び出す。
   **正直な開示**: `cg-cad-core`自身が敷地・建蔽率・階数から独自に
   footprintを再算出する設計のため、間取り案の`footprint_m2`と厳密には
   一致しない近似(「同一の敷地条件・階数から`cg-cad-core`が生成する
   代表的な建物ボリューム」に対する構造計算)。`open-cg-cad`はまだ
   HTTP APIを公開していないため、実プロセス間HTTP連携ではなくcrate
   直接依存を採用した。
4. **LLMコメント生成**(`koumuten-core::llm_comment`): `aruaru-llm`の
   `POST /v1/generate`(GPT-2 124M)を呼び、ユーザー向けの自然文コメント
   (「この様なデザインで素敵なお家を立ててみました。」+生成文)を作る。
   接続失敗・エラー応答時は定型文へフォールバックする(実際に
   `aruaru-llm`未起動状態で動作確認済み)。
5. **免責文言**(`koumuten_core::DISCLAIMER_JA`): 全APIレスポンスの
   `disclaimer`フィールドに必ず含める。一級建築士等の有資格者による
   最終確認前のAI提案であることを明記。

## 検証結果

- `cargo build --workspace --release` — 成功(警告なし)。
- `cargo test --workspace --release` — `koumuten-core`16件+
  `koumuten-api`7件、**計23件全て成功**(建蔽率超過案の生成→除外・
  接道義務未指定時のスキップ・採光違反検出・構造計算の成功/失敗ケース・
  LLM接続失敗時のフォールバック等)。
- 実サーバー起動+PowerShellの`Invoke-WebRequest`による実HTTP検証 —
  `/healthz`・`/v1/proposals`(`aruaru-llm`未起動時のフォールバック応答、
  建蔽率上限超過案が`structural_check`をスキップして正しく除外される
  ことを含む)を確認済み。
- **実際に`aruaru-llm`(GPT-2 124M実重み)を起動した状態でも
  `/v1/proposals`を実行し、フォールバックではない実際のLLM生成コメント
  (`"この様なデザインで素敵なお家を立ててみました。..."`)が返ることを
  確認済み**。

## 見送った部分(次フェーズ、今回は一切実装していない)

- **電子契約**: 未実装。今回のタスク指示で明示的にスコープ外とされた。
- **決済**: 未実装。同上。
- **簿記**: 未実装。同上。
- **建築基準法の網羅的チェック**: 高さ制限・日影規制・斜線制限(道路
  斜線・隣地斜線・北側斜線)・防火地域規制・避難規定(廊下幅・階段・
  排煙等)は未実装(`legal_check`モジュールの`not_checked_items`で
  APIレスポンスにも明記)。
- **実際の間取り設計**: 動線計画・窓の配置・耐火構造の詳細等は行わない
  (グリッド分割の単純な矩形配置に留まる)。
- **open-fudousanへの変更**: 今回は一切行っていない(タスク指示通り)。
- **実プロセス間HTTPでのopen-cg-cad連携**: `open-cg-cad`側にHTTP APIが
  無いため、今回はcrate直接依存(同一プロセス内呼び出し)に留めた。
