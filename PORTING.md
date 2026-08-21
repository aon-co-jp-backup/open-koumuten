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
