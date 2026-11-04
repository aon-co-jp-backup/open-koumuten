# PORTING (open-koumuten)

他プロジェクトへこのリポジトリの構成パターンを移植する場合のメモ。

## 依存するsibling repository(path依存、CI/他マシンでの再現に必要)

`open-koumuten`は同一の親ディレクトリ(`F:\runo`)に以下がcloneされて
いることを前提とする(`Cargo.toml`のpath依存):

```
F:\runo\open-koumuten\   (このリポジトリ)
F:\runo\open-cg-cad\     (koumuten-core -> cg-cad-core への path依存)
F:\runo\open-cuda\       (cg-cad-core自体がこれをpath依存、間接依存)
```

`aruaru-llm`は**HTTP経由**での連携(`koumuten-core::llm_comment`)のため
path依存は不要——ただし実際にLLM生成コメントを試すには
`F:\runo\aruaru-llm`をビルド・起動しておく必要がある(既定URL
`http://127.0.0.1:4600`、`KOUMUTEN_ARUARU_LLM_URL`で変更可)。
`open-fudousan`との連携もHTTP経由(`open-koumuten`側が`open-fudousan`の
`export-for-cad`出力と互換のJSONを受け取る設計)のため、こちらもpath依存
不要。

## ビルド手順

```sh
cd F:\runo\open-koumuten
cargo build --workspace --release
cargo test --workspace --release
```

## サーバー起動手順

```sh
# 1. (任意) aruaru-llmを起動しておくと実際のLLMコメントが得られる
cd F:\runo\aruaru-llm
./target/release/aruaru-llm.exe

# 2. open-fudousanを起動し、敷地情報を取得しておく(任意、
#    手動でJSONを組み立てても良い)
cd F:\runo\open-fudousan
./target/release/fudousan-api.exe
curl http://127.0.0.1:8080/v1/properties/dummy-001/export-for-cad

# 3. open-koumuten本体を起動
cd F:\runo\open-koumuten
./target/release/koumuten-api.exe
curl -X POST http://127.0.0.1:8090/v1/proposals -H "Content-Type: application/json" -d @site.json
```

`site.json`は`open-fudousan`の`export-for-cad`レスポンスに
`road_frontage_m`(道路に接する辺の長さ、メートル、任意)を追加した
JSONファイル(README.md「入力形式」参照)。

## 移植時の注意点(このリポジトリ自身の設計判断の記録)

- `koumuten-core`は`open-cg-cad`をcrate直接依存(pathベース)として
  呼び出している——`open-cg-cad`がまだHTTP APIを公開していないため。
  将来`open-cg-cad`側にHTTP APIが実装されたら、`structural_check`
  モジュールをHTTP経由の呼び出しへ置き換える余地がある(その場合は
  `open-cg-cad`へのpath依存を削除できる)。
- `koumuten-core::site_input::SiteInput`は`open-fudousan`の
  `CadExport`とフィールド互換だが、`road_frontage_m`という本リポジトリ
  独自の追加フィールドを持つ——`open-fudousan`側は変更していないため、
  `open-fudousan`から直接返ってくるJSONにはこのフィールドが無く、
  `serde(default)`によりデフォルトの`None`として扱われる(呼び出し側が
  手動で追加する必要がある)。

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

### Deutsch (KI-Übersetzung, noch nicht von Muttersprachlern geprüft)
- **REST wird abgeschafft. Neue APIs werden nicht als REST gebaut.** APIs, die aus dem Browser oder mit `curl` aufgerufen werden, nutzen **Connect-Unary-Aufrufe** (`POST /<Paket>.<Service>/<Methode>`, `content-type: application/json`, JSON-Body); zwischen Diensten gilt **gRPC / gRPC-web** (wie bei der Admin-API von RPoem, `open_runo.admin.v1.Admin`). Streaming erst, wenn es wirklich nötig ist.
- **Bestehende REST-Endpunkte werden einzeln migriert, sobald man sie anfasst.** Nach der Migration wird die alte REST-Route entfernt. Bis dahin bleiben sie bestehen, es werden aber **keine neuen hinzugefügt**.
- **JSON wird auf RS-JSON vereinheitlicht**: Lesen mit `from_str_secure` / `parse_secure` (Grenzen für Größe, Tiefe und doppelte Schlüssel), Schreiben mit `rust_json::fast`. `prost` kommt nicht in Browser-WASM (die Datei würde größer).
- **Ausnahmen (Formate, die ein fremder Standard vorgibt, nicht unsere eigene API)**: ACME, OAuth/OIDC-Callbacks, Webhooks von Dritten (z. B. GitHub), Health-Checks, statische Dateien, OTLP usw. Diese nehmen schlichtes HTTP entgegen, wie es der Standard verlangt.
- Referenzimplementierungen: `RPoem` (`open_runo.admin.v1.Admin`) und `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Hinweis: Ob der Code eines Repositorys diese Richtlinie schon befolgt, hängt vom jeweiligen Repository ab. Dieser Abschnitt hält die Richtlinie fest; er bedeutet nicht, dass der Code bereits geändert wurde.

### Русский (машинный перевод, носителями языка ещё не проверен)
- **REST полностью отменяется. Новые API не создаются как REST.** API, вызываемые из браузера или через `curl`, используют **унарные вызовы Connect** (`POST /<пакет>.<Service>/<Method>`, `content-type: application/json`, тело — JSON); между сервисами — **gRPC / gRPC-web** (так же, как административный API RPoem, `open_runo.admin.v1.Admin`). Потоковая передача — только когда она действительно нужна.
- **Существующие REST-эндпоинты переносятся по одному, когда до них доходят руки.** После переноса старый REST-маршрут удаляется. До завершения переноса они остаются, но **новые не добавляются**.
- **JSON унифицируется на RS-JSON**: чтение через `from_str_secure` / `parse_secure` (ограничения на размер, глубину и повторяющиеся ключи), запись через `rust_json::fast`. `prost` не добавляется в браузерный WASM (размер вырос бы).
- **Исключения (форматы, заданные чужим стандартом, а не наш собственный API)**: ACME, обратные вызовы OAuth/OIDC, вебхуки сторонних сервисов (например, GitHub), проверки работоспособности, раздача статических файлов, OTLP и т. п. Они принимают обычный HTTP, как требует стандарт.
- Эталонные реализации: `RPoem` (`open_runo.admin.v1.Admin`) и `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Примечание: соответствует ли код конкретного репозитория этой политике, зависит от самого репозитория. Этот раздел фиксирует политику и не означает, что код уже изменён.
