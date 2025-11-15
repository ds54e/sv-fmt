# sv-fmt

Rust 製の SystemVerilog フォーマッタです。sv-mint の設定思想を踏襲しつつ、解析からテキスト生成までを単一プロセスで完結させることで、高速かつシンプルなワークフローを提供します。

## 特長

- **Rust ネイティブ**: `sv-parser` による CST 解析と formatter を同プロセスで実行。IPC や Python 依存がなく、CI やエディタ統合にそのまま組み込めます。
- **基本フォーマットルール**:
  - インデント正規化（タブ/スペース切替、`indent_width` 指定）
  - カンマや関数呼び出しスペースの調整、`end else` の同一行化
  - プリプロセッサディレクティブの左寄せ
  - `wrap_multiline_blocks=true` 時、複数文を含む `if/else/for/...` に `begin...end` を自動挿入
- `package`/`class`/`interface` 宣言の直前に空行を追加し、コメントを崩さずブロックを視覚的に分離
  - 行末空白除去と終端改行の強制
- **CLI サポート**: `--check` でフォーマット差分のみ検出、`-i/--in-place` で上書き可。複数ファイル/ディレクトリ入力や `sv-fmt.toml` による設定上書きにも対応。

## 使い方

```bash
# 標準出力へフォーマット結果を出力
sv-fmt path/to/file.sv

# ディレクトリを再帰的に走査して上書き
sv-fmt -i rtl/ ip/

# フォーマット差分があるかのみ検査（CI 向け）
sv-fmt --check rtl/top.sv

# カスタム設定ファイルを指定
sv-fmt --config ./sv-fmt.toml rtl/
```

### オプション

| オプション | 説明 |
|------------|------|
| `FILES...` | ファイルまたはディレクトリを指定（複数可） |
| `-i`, `--in-place` | 入力ファイルを上書き |
| `--check` | フォーマットが必要な場合に非 0 で終了、差分は表示しない |
| `--config <PATH>` | `sv-fmt.toml` のパスを指定 |

## ライブラリとして利用する

CLI と同じフォーマッタを、クレート `sv_fmt` としてライブラリ経由で呼び出すこともできます。プラグイン的に統合したい場合は以下のように利用してください。

```rust
use sv_fmt::{config::FormatConfig, formatter::format_text};

fn format_sv(source: &str) -> anyhow::Result<String> {
    let cfg = FormatConfig::default();
    let formatted = format_text(source, &cfg)?;
    Ok(formatted)
}
```

内部モジュールは字句解析・レイアウト・改行制御に分割されているため、今後 API を追加したり、独自ルールを組み合わせたい場合でも追従が容易です。

## 設定 (`sv-fmt.toml`)

sv-mint と同等のキーを TOML で定義します。存在しない場合は組み込みデフォルトが使われます。

```toml
indent_width = 2
use_tabs = false
align_preprocessor = true
wrap_multiline_blocks = true
inline_end_else = true
space_after_comma = true
remove_call_space = true
max_line_length = 100
align_case_colon = true
auto_wrap_long_lines = false
```

- `indent_width`, `use_tabs`: インデント幅とタブ使用有無
- `align_preprocessor`: `ifdef` などのディレクティブを左端に揃える
- `wrap_multiline_blocks`: 複数行の `if/else/for/...` に `begin...end` を補完
- `inline_end_else`: `end` の直後の `else` を同一行に配置
- `space_after_comma`: カンマ後スペース強制、直前スペース除去
- `remove_call_space`: 関数/タスク呼び出し名と `(` の間のスペースを削除
- `max_line_length`: フォーマット後の行幅上限。越えた場合は `--check` だけでなく通常実行でもエラーで知らせる
- `align_case_colon`: `case`/`casez`/`casex` のラベル `:` を列揃えする
- `auto_wrap_long_lines`: `max_line_length` を超えた行をカンマ/空白位置で折り返す試験的ヒューリスティック

プロジェクトに合わせて調整できるサンプル設定は `sv-fmt.example.toml` にまとまっています。必要に応じて `sv-fmt.toml` としてコピーし、コメントを参考に値を書き換えてください。

## ライセンス

MIT License
