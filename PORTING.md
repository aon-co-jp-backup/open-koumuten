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
