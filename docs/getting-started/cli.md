# コマンドライン

以下の`rusk`は、ビルドした`./target/release/rusk`を表します。`compile`・`dependencies`は入力ファイルを一つ受け取ります。`compile`のオプションはファイル名の前後に指定できます。

```text
rusk compile [OPTIONS] FILE
rusk dependencies FILE
rusk fmt [--check | --write] FILE...
rusk search QUERY [--file FILE] [--stdlib] [--all] [--json]
rusk doc [--file FILE] [--stdlib] [--all] [--json | --source-url URL]
rusk --help
rusk compile --help
rusk dependencies --help
rusk fmt --help
rusk search --help
rusk doc --help
rusk --version
```

## C++を生成する

```bash
rusk compile main.rusk -o main.cpp
```

| オプション | 意味 |
|---|---|
| `-o PATH` / `--output PATH` | 出力先。省略時はカレントディレクトリの`a.cpp` |
| `--emit STAGE` | 出力する段階。省略時は`cpp` |
| `--no-opt` | 標準で有効な最適化を無効化して比較する |
| `--help` / `-h` | 指定したコマンドの使い方を表示する |

`compile`はC++コンパイラを起動しません。実行ファイルの作成は別途`g++`などで行います。構文・型検査などに失敗すると診断を標準エラーへ表示し、正常終了しません。以前生成したファイルが残る場合があるため、失敗後に古い生成物を実行しないようにしてください。

## ヘルプ・引数・終了コード

`rusk --help`はコマンドの一覧、`rusk compile --help`・`rusk dependencies --help`は各コマンドの引数とオプションを表示します。`rusk help compile`も使えます。`rusk --version`は処理系のパッケージバージョンを表示します。

`--emit=hir`・`--output=main.hir`のように値を`=`で指定できます。同じ出力先・出力段階を複数回指定した場合は最後の値を使い、`--no-opt`の重複指定も許可します。

パスに空白がある場合はシェルで引用符を付けます。`-`で始まるファイル名は、オプションを終える`--`の後に置けます。

```bash
rusk compile --emit hir --output "generated file.hir" "main program.rusk"
rusk compile -o main.cpp -- -source.rusk
```

| 終了コード | 意味 |
|---|---|
| `0` | 変換・整形・依存一覧・API検索・文書生成、またはヘルプ・バージョン表示の成功 |
| `1` | ソースの構文・型検査、ファイルの読み書きなどの失敗、または`fmt --check`で整形の差を検出 |
| `2` | 引数不足、不明なコマンド・オプション、不正な`--emit`の値など、コマンドラインの指定ミス |
| `101` | 処理系のpanicによる終了。通常のビルドでのコードであり、abortやシグナルによる終了は別 |

ヘルプ・バージョンは標準出力、引数エラーと処理中の診断は標準エラーに表示します。引数なしの実行は、標準エラーにヘルプを表示して終了コード2を返します。引数エラー時はソースを読み込まず、生成ファイルを作りません。

panicが発生した場合は`Rusk internal compiler error`に続けて、パッケージバージョン、処理段階、原因、Rust側の発生位置を表示します。`RUST_BACKTRACE=1 rusk compile main.rusk`でバックトレースも付けられます。通常のソースエラーやファイルI/Oの失敗は、内部エラー表示を使いません。内部エラーが出た場合は、診断と再現するソースを添えて報告してください。panic後は処理を継続しません。

## ソースを整形する

`rusk fmt main.rusk`は標準出力へ整形結果を表示し、`--write`で入力ファイルを更新、`--check`で整形済みかを確認します。型検査とinclude先の読み込みは行いません。複数ファイルには`--check`か`--write`が必要です。規則と更新時の扱いは[ソースの整形](formatting.md)を参照してください。

## ライブラリを検索する { #library-search }

```bash
rusk search sort
rusk search "現在の走査位置"
rusk search get --file library.rusk --stdlib --json
```

名前、署名、文書コメントを大文字小文字の区別なしで部分一致検索し、署名・説明の冒頭・定義位置を表示します。名前の完全一致、名前の前方一致、名前・署名の部分一致、説明の一致の順に並べます。同じ優先度では名前・署名・定義位置の順です。一致がなければ空の結果で正常終了します。

既定の対象は組み込み標準ライブラリです。`--file`を指定すると、そのファイルと相対includeで読み込むRuskソースを対象にします。`--stdlib`を併記すると両方を検索します。`--all`で標準の言語フック、導出宣言、ランタイムsymbolも含めます。

## APIリファレンスを生成する { #api-docs }

```bash
rusk doc > api.md
rusk doc --file library.rusk --json > api.json
```

`doc`は検索と同じ宣言情報を、ソースファイル順にMarkdownで出力します。型・関数・extern・フィールド・variantを区別し、オーバーロードは個別に表示します。`--source-url`は標準ライブラリのソースリンクの基準URLです。`--json`は宣言の種類、署名、型引数、引数注釈、説明、定義位置、識別子を含む機械可読形式を出力します。`search --json`でも各検索結果は同じ形式です。

いずれも構文とincludeを読み取ります。型検査、ジェネリックの具体化、C++資源の読み込みや実行は行いません。文書コメントの記法は[文書コメント](../development/doc-comments.md)を参照してください。

## 中間表現を調べる

```bash
rusk compile main.rusk --emit hir -o /tmp/main.hir
rusk compile main.rusk --emit lir -o /tmp/main.lir
```

| 段階 | 主な内容 |
|---|---|
| `cst` | 入力ソースの構文構造 |
| `ast` | ASTの構造。演算子の優先順位などを反映 |
| `hir` | 型や参照先を解決した中間表現 |
| `lir` | 基本ブロックと分岐を持つ中間表現 |
| `cpp` | 実行用のC++ソース |

`--emit`を変えても既定の出力ファイル名は`a.cpp`です。CST・ASTの出力は調査用であり、型検査が成功したことの代わりにはなりません。各段階の役割は[変換パイプライン](../development/pipeline.md)に説明しています。

## includeの依存先を調べる

```bash
rusk dependencies main.rusk
```

読み込み対象のRuskソースと、`extern symbol`の`code_file`・`code_files`で参照するC++ファイルのパスを出力します。依存ファイルの確認に使うコマンドで、プログラム全体の型検査や実行は行いません。ファイル分割の書き方は[include](../reference/records.md)を参照してください。
