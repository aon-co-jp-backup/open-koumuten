# 開発方針＆開発環境ルール(open-koumuten)

AI工務店(仮称)。**AI間取り提案・違法性チェックシミュレーション**
(段階的開発の第1フェーズ)を行うRust製MVP。姉妹リポジトリ
`F:\runo\open-fudousan`(土地・物件検索)から敷地情報を受け取り、
`F:\runo\open-cg-cad`(構造計算・耐震シミュレーション)・
`F:\runo\aruaru-llm`(LLM推論サーバー)と連携する。

エコシステム共通の開発ルール(Rust中心、実機検証必須、正直な開示徹底、
新規フォルダ作成前の相談ルール、DB非依存・1バイナリ完結・テンプレート
エンジン不使用等)は
[`open-raid-z`](https://github.com/aon-co-jp/open-raid-z)の`CLAUDE.md`が
**正本**であり、詳細をここに複製せずそちらを参照する。

## ⚠️ 最重要制約(必ず全ドキュメント・コード・APIレスポンスに一貫して
明記すること)

**このシステムが生成する間取り提案は、あくまで一級建築士など人間の
有資格者による最終確認の一歩手前までを代行するドラフト/サンプル生成
ツールである。「法的に確定した設計」「建築確認申請に使える正式な図面・
仕様書」を生成するものでは断じてなく、必ず有資格者のレビューが必要
という前提を省略・弱化しないこと。** この免責事項は`koumuten-core`
クレートのルートdocコメント(`src/lib.rs`)・全APIレスポンスの
`disclaimer`フィールド・README.md/README-English.mdの全てに明記済み。

## 今回のスコープ(段階的開発、ユーザーの明示的な優先指示)

「AI間取り提案・違法性チェックシミュレーション」機能**のみ**に限定。
**電子契約・決済・簿記機能は次フェーズであり、今回は一切実装していない**
(スコープに含めない、という明示的な指示)。次回以降のセッションで
これらに着手する場合も、まずユーザーへ確認すること(勝手にスコープを
広げない)。

## 構成

```
open-koumuten/
├── Cargo.toml (workspace)
├── crates/
│   ├── koumuten-core/   # 間取り生成・違法性チェック・構造連携・LLM連携のコアロジック
│   └── koumuten-api/    # tokio/hyper直接実装のHTTP API
```

## 連携先プロジェクトとの連携方針

- **open-fudousan**(`F:\runo\open-fudousan`): 敷地情報の提供元。
  今回は**変更していない**(タスク指示通り)。`koumuten-core::site_input`
  が`open-fudousan`の`export-for-cad`エンドポイント出力と互換の入力
  モデルを新規に用意し、受信専用として扱う。`open-fudousan`側に
  存在しない接道義務チェック用フィールド`road_frontage_m`を、本
  リポジトリ側の追加提案として新設した(`open-fudousan`側は無変更、
  詳細は`site_input.rs`のドキュメント参照)。
- **open-cg-cad**(`F:\runo\open-cg-cad`): 構造計算・耐震シミュレーション
  の提供元。`open-cg-cad`は現時点でHTTP APIを公開していない
  (`cg-cad-core`クレートのみ)ため、**crate直接依存(path依存)**を
  採用した(タスク指示の「実プロセス間HTTP、または直接crate依存の
  どちらか実現しやすい方でよい」に基づく判断)。`generate_building`+
  `analyze_structure`をそのまま呼び出す。
- **aruaru-llm**(`F:\runo\aruaru-llm`): LLM推論サーバー。**実プロセス間
  HTTP**(`reqwest` blocking、`POST /v1/generate`)で連携する。接続失敗時は
  定型文へフォールバックし、`aruaru-llm`未起動でも動作確認できる設計
  (タスク指示通り)。

## HANDOFF

- **2026-08-23 CPU SIMD 適用可否の調査(結論: 適用対象外、コード変更なし)**:
  エコシステム共通の CPU 命令セット検出・ディスパッチ基盤
  [`open-cpu`](https://github.com/aon-co-jp/open-cpu) の拡張に伴い、
  本リポジトリにも適用先があるかを調査した。
  **結論: 本リポジトリ自身には適用対象になる処理は無い。**

  理由(実コードを確認した上での判断):
  - Rust コードは全体で約 1150 行。`koumuten-core`(間取り提案・
    法規チェック・構造チェック・LLM コメント)と `koumuten-api`
    (hyper 直叩きの HTTP、ポート 8090)の 2 crate。
  - `floor_plan.rs` の探索は階数候補 × 建蔽率候補の二重ループだが
    組み合わせ数は十数通り。`layout_rooms` も 1 フロア 6 部屋程度。
    `legal_check.rs` のループも要素数が 2 桁未満。
  - 数値計算らしい処理(構造解析)は `structural_check.rs` から
    `cg-cad-core::analyze_structure` へ委譲しており、**本リポジトリ内で
    計算していない**。したがって高速化するなら `open-cg-cad` 側で行うのが
    正しい(実際、今回 `open-cg-cad` 側でトンネル微気圧波指標の
    O(n^2) → O(n) 改善を実施した。詳細は `open-cg-cad/CLAUDE.md` 参照)。

  **無理に SIMD を入れることはしない**という判断を明記しておく。
  `open-cpu` への依存も追加していない。
  - 次にすべきこと: 間取り探索を全数探索から本格的な最適化(遺伝的
    アルゴリズム等)へ拡張し、候補数が数万オーダーになった段階で
    初めてベクトル化が検討対象になる。その時点で本節を見直すこと。


- **2026-08-21 新規作成・段階1(間取り提案・違法性チェック)実装完了**:
  - **構成**: `Cargo.toml`(workspace)+`crates/koumuten-core`
    (`site_input`/`floor_plan`/`legal_check`/`structural_check`/
    `llm_comment`の5モジュール)+`crates/koumuten-api`
    (`koumuten-api`、tokio/hyper直接実装、fudousan-apiと同じ
    `route()`純粋関数パターンで単体テスト可能)。README.md/
    README-English.md/PORTING.md/本CLAUDE.mdを新規作成。
  - **間取り生成**(`floor_plan.rs`): 敷地の外接矩形・建蔽率・容積率・
    階数の組み合わせから複数案を生成。部屋配置はfootprintの単純グリッド
    分割(LDK・寝室・浴室・トイレ・玄関・収納)。意図的に建蔽率を超過
    する案も生成し(`GenerationOptions::default`の`footprint_coverage_
    fractions`に`1.15`を含む)、`legal_check`が正しく除外できることを
    実証する設計にした。
  - **違法性チェック**(`legal_check.rs`): 3項目のみ実装(全ての建築
    基準法細目は網羅しない、正直な開示): (1) 建蔽率・容積率
    (`cg-cad-core::zoning::check_zoning_compliance`を再利用)、
    (2) 接道義務の簡易判定(`road_frontage_m >= 2.0m`のみ、道路幅員・
    種別は見ない)、(3) 採光規定の明らかな違反の簡易判定(居室が
    footprintの外壁に一切接していない場合のみ検出)。`checked_items`/
    `not_checked_items`フィールドで実装範囲をAPIレスポンス自体にも
    正直に明記する設計にした。
  - **構造連携**(`structural_check.rs`): `cg-cad-core`をpath依存として
    直接呼び出し(`generate_building`→`analyze_structure`)。
    `cg-cad-core`自身が敷地・建蔽率・階数からfootprintを再算出するため、
    間取り案自体の`footprint_m2`とは厳密には一致しない近似であることを
    モジュールdocコメントに明記。
  - **LLM連携**(`llm_comment.rs`): `aruaru-llm`の`POST /v1/generate`を
    呼び、失敗時は定型文へフォールバック。`open-cg-cad::llm_explain`と
    同じ設計パターン(英語プロンプト、GPT-2 124Mの語彙特性に合わせる)。
  - **免責事項**: `koumuten_core::DISCLAIMER_JA`を全APIレスポンスの
    `disclaimer`フィールドに必ず含める設計にし、実際にテスト
    (`proposals_endpoint_returns_disclaimer_and_entries`)で検証済み。
  - **検証結果**: `cargo build --workspace --release`成功(警告なし)。
    `cargo test --workspace --release`**23件全green**
    (`koumuten-core` 16件+`koumuten-api` 7件)。実際に
    `koumuten-api.exe`を起動し、PowerShellの`Invoke-WebRequest`で
    `/healthz`・`/v1/proposals`(`aruaru-llm`未起動時のフォールバック
    応答含む)を確認。さらに**実際に`aruaru-llm.exe`
    (実GPT-2 124M重み、ポート4600)を起動した状態でも`/v1/proposals`を
    実行し、フォールバックではない実際のLLM生成コメント
    (`"この様なデザインで素敵なお家を立ててみました。..."`)が返る
    ことを実HTTPで確認済み**(型チェック・ビルド成功だけで完了と
    報告しない方針の実践)。
  - **見送った部分(正直な開示)**: 電子契約・決済・簿記(タスク指示で
    明示的にスコープ外とされた次フェーズ)、建築基準法の網羅的チェック
    (高さ制限・日影規制・斜線制限・防火地域規制・避難規定は未実装)、
    実際の間取り設計(動線計画・窓配置の詳細等)、`open-fudousan`への
    変更(今回は一切行っていない)、`open-cg-cad`との実プロセス間HTTP
    連携(`open-cg-cad`側にHTTP APIが無いため、crate直接依存に留めた)。
  - **git commit**: ローカルコミットのみ実施、**pushは行っていない**
    (タスク指示通り、GitHubにリポジトリを新規作成する権限もないため)。
  - 次にすべきこと: (1) ユーザーの許可を得た上で、GitHubに
    `aon-co-jp/open-koumuten`リポジトリを作成しpushする(権限確認が
    必要)、(2) `open-cg-cad`側にHTTP APIが実装されたら
    `structural_check`をHTTP経由の呼び出しへ置き換える余地の検討、
    (3) 建築基準法チェック項目の拡充(高さ制限・斜線制限等、優先度は
    ユーザーへ確認)、(4) 次フェーズ(電子契約・決済・簿記)は
    ユーザーの明示的な指示を待ってから着手する。

## API 方針: REST 全廃・gRPC / Connect へ (2026-11-04)

### 日本語
- **REST は全廃する。新しい API は REST で作らない。** ブラウザや `curl` から呼ぶ API は **Connect の単純呼び出し**(`POST /<パッケージ>.<Service>/<Method>`、`content-type: application/json`、本文は JSON)、サービス間は **gRPC / gRPC-web**(RPoem の管理API `open_runo.admin.v1.Admin` と同じ流儀)。ストリーミングは、必要になってから。
- **既存の REST は、触るときに順に移す。** 移したら旧 REST の経路は消す。移行が終わるまで残るが、**新規には追加しない**。
- **JSON は RS-JSON に統一**する(読み込みは `from_str_secure` / `parse_secure` でサイズ・深さ・重複キーの上限つき、書き出しは `rust_json::fast`)。ブラウザの WASM に `prost` は入れない(サイズが増えるため)。
- **例外(自分たちの API ではなく、相手の規格が決めているもの)**: ACME、OAuth/OIDC のコールバック、第三者(GitHub など)からの Webhook、ヘルスチェック、静的ファイル配信、OTLP など。これらは規格どおりの HTTP を受ける。
- 参考実装: `RPoem`(`open_runo.admin.v1.Admin`)、`open-easy-web`(`server/src/connect.rs`、`proto/open_easy_web/admin/v1/admin.proto`)。
- ※ このリポジトリのコードが既に方針どおりかどうかは、各リポジトリの現状による。この節は方針の記録であり、コードの変更を意味しない。

### English
- **REST is being abolished. New APIs are not built as REST.** APIs called from browsers or `curl` use **Connect unary calls** (`POST /<package>.<Service>/<Method>`, `content-type: application/json`, JSON body); service-to-service calls use **gRPC / gRPC-web** (the same style as RPoem's admin API `open_runo.admin.v1.Admin`). Streaming only when it is actually needed.
- **Existing REST endpoints are migrated one by one, when they are touched.** The old REST route is removed once migrated. Until migration is finished they remain, but **no new ones are added**.
- **JSON is unified on RS-JSON**: reading with `from_str_secure` / `parse_secure` (size, depth and duplicate-key limits), writing with `rust_json::fast`. `prost` is not put into browser WASM (it would grow the binary).
- **Exceptions (shapes dictated by someone else's standard, not our own API)**: ACME, OAuth/OIDC callbacks, webhooks from third parties (e.g. GitHub), health checks, static file serving, OTLP, and so on. These accept plain HTTP as the standard requires.
- Reference implementations: `RPoem` (`open_runo.admin.v1.Admin`) and `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Note: whether the code in a given repository already follows this policy depends on that repository. This section records the policy; it does not mean the code has been changed.

### Deutsch (KI-Übersetzung, redaktionell überarbeitet; noch nicht von Muttersprachlern geprüft)
- **REST wird vollständig abgeschafft; neue APIs entstehen nicht mehr als REST.** APIs, die aus dem Browser oder per `curl` aufgerufen werden, verwenden **unäre Connect-Aufrufe** (`POST /<Paket>.<Service>/<Methode>`, `content-type: application/json`, JSON-Body). Zwischen Diensten kommen **gRPC bzw. gRPC-web** zum Einsatz, im selben Stil wie die Admin-API von RPoem (`open_runo.admin.v1.Admin`). Streaming wird erst ergänzt, wenn es tatsächlich gebraucht wird.
- **Bestehende REST-Endpunkte werden schrittweise umgestellt, und zwar immer dann, wenn man ohnehin an ihnen arbeitet.** Sobald ein Endpunkt umgestellt ist, wird die alte REST-Route entfernt. Bis zum Abschluss der Migration bleiben die übrigen bestehen, es werden aber **keine neuen mehr hinzugefügt**.
- **JSON wird einheitlich über RS-JSON verarbeitet**: Gelesen wird mit `from_str_secure` bzw. `parse_secure` (mit Obergrenzen für Größe, Verschachtelungstiefe und doppelte Schlüssel), geschrieben mit `rust_json::fast`. `prost` wird nicht in das Browser-WASM aufgenommen, da es die Datei vergrößern würde.
- **Ausnahmen (Schnittstellen, deren Form ein fremder Standard vorgibt und die nicht zu unserer eigenen API gehören)**: ACME, OAuth/OIDC-Callbacks, Webhooks von Drittanbietern (z. B. GitHub), Health-Checks, die Auslieferung statischer Dateien, OTLP und Ähnliches. Diese nehmen, wie vom jeweiligen Standard verlangt, gewöhnliches HTTP entgegen.
- Referenzimplementierungen: `RPoem` (`open_runo.admin.v1.Admin`) und `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Hinweis: Ob der Code eines einzelnen Repositorys diese Richtlinie bereits erfüllt, hängt vom jeweiligen Repository ab. Dieser Abschnitt dokumentiert lediglich die Richtlinie; er besagt nicht, dass der Code schon angepasst wurde.

### Русский (перевод выполнен с помощью ИИ и отредактирован; носителями языка пока не проверен)
- **От REST мы полностью отказываемся; новые API больше не создаются в стиле REST.** API, к которым обращаются из браузера или через `curl`, используют **унарные вызовы Connect** (`POST /<пакет>.<Service>/<Method>`, `content-type: application/json`, тело запроса — JSON). Для взаимодействия между сервисами применяются **gRPC и gRPC-web** — в том же стиле, что и административный API RPoem (`open_runo.admin.v1.Admin`). Потоковая передача данных добавляется лишь тогда, когда в ней действительно возникает необходимость.
- **Существующие REST-эндпоинты переводятся поэтапно — по мере того, как с ними приходится работать.** После перевода старый REST-маршрут удаляется. Пока миграция не завершена, оставшиеся эндпоинты сохраняются, но **новые не добавляются**.
- **Работа с JSON унифицирована на основе RS-JSON**: чтение — через `from_str_secure` или `parse_secure` (с ограничениями на размер, глубину вложенности и повторяющиеся ключи), запись — через `rust_json::fast`. `prost` в браузерный WASM не включается, поскольку это увеличило бы размер файла.
- **Исключения (интерфейсы, форму которых определяет чужой стандарт, а не наш собственный API)**: ACME, обратные вызовы OAuth/OIDC, вебхуки сторонних сервисов (например, GitHub), проверки работоспособности, раздача статических файлов, OTLP и тому подобное. Такие интерфейсы принимают обычный HTTP, как того требует соответствующий стандарт.
- Эталонные реализации: `RPoem` (`open_runo.admin.v1.Admin`) и `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Примечание: соответствует ли код конкретного репозитория этой политике, зависит от самого репозитория. Данный раздел лишь фиксирует политику и не означает, что код уже изменён.
